use std::ptr::null;
use influxdb::{WriteQuery, InfluxDbWriteable};

use crate::{config, csi, telemetry};
use crate::error::RecvMessageError;
use crate::message::{MessageData, MessageType};

use std::sync::Arc;

use dashmap::DashMap;
use ndarray::{Array, Axis};
use num::range;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use crate::csi::{CSIReading, CSIStore};

fn csi_metrics_measurement() -> String {
    config::get().lock().unwrap().influx.csi_metrics_measurement.clone()
}

pub fn handle_message(m: MessageData, f: &Arc<DashMap<String, CSIStore>>) -> Result<Vec<WriteQuery>, RecvMessageError> {
    match m.format {
        MessageType::Telemetry => handle_telemetry(m),
        MessageType::CSI => handle_csi(m, f),
        MessageType::CSICompressed => handle_compressed_csi(m, f)
    }
}

fn handle_telemetry(message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let reading = parse_telemetry(&message.payload)?;
    Ok(vec![reading.into_query(&config::get().lock().unwrap().influx.sensor_telemetry_measurement)])
}

fn parse_telemetry(expected_payload: &[u8]) -> Result<telemetry::TelemetryReading, RecvMessageError> {
    let protobuf_parse_result = telemetry::parse_telemetry_protobuf(expected_payload)?;
    Ok(telemetry::get_reading(&protobuf_parse_result))
}

fn handle_csi(message: MessageData, f: &Arc<DashMap<String, CSIStore>>) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let frame = parse_csi(&message.payload)?;
    let mapped_reading = map_reading(frame, f);

    Ok(vec![mapped_reading.into_query(csi_metrics_measurement())])
}

fn parse_csi(expected_payload: &[u8]) -> Result<CSIReading, RecvMessageError>  {
    let frame = csi::parse_csi_protobuf(&expected_payload)?;
    csi::get_reading(&frame)
}

fn handle_compressed_csi(message: MessageData, f: &Arc<DashMap<String, CSIStore>>) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let mut write_queries: Vec<WriteQuery> = Vec::new();

    let compressed_frame_size = (config::get().lock().unwrap().message.csi_frame_size + 1) as usize;

    // batch of readings
    let expected_compressed_size = u16::from_le_bytes(message.payload[0 .. 2].try_into().unwrap());
    let expected_end_index = (expected_compressed_size + 2) as usize;
    //println!("Compressed CSI container with expected_size: {} actual size: {}", expected_compressed_size, message.payload.len() - 2);

    let compressed_payload = message.payload[2 .. expected_end_index].to_vec();
    let decompressed_data = inflate::inflate_bytes_zlib(compressed_payload.as_slice()).unwrap();
    let frame_count = decompressed_data.len() / compressed_frame_size;

    if (decompressed_data.len() % compressed_frame_size) > 0 {
        println!("Could not determine the number of frames in compressed container from {:?} with size: {:?}.", message.addr, decompressed_data.len());
        return Err(RecvMessageError::MessageDecompressionError())
    }

    // println!("Frames in container: {:?} from {}", frame_count, message.addr);

    for i in 0 .. frame_count {
        let protobuf_size = decompressed_data[compressed_frame_size * i] as usize;

        let protobuf_start = (compressed_frame_size * i) + 1;
        let protobuf_end = protobuf_start + protobuf_size;
        let protobuf_contents = &decompressed_data[ protobuf_start .. protobuf_end ];

        let Ok(msg) = csi::parse_csi_protobuf(protobuf_contents) else {
            println!("Invalid frame in decompressed array.");
            continue
        };

        let Ok(reading) = csi::get_reading(&msg) else {
            println!("Invalid frame in decompressed array.");
            continue
        };

        let mapped_reading = map_reading(reading, f);
        write_queries.push(mapped_reading.into_query(csi_metrics_measurement()));
    }

    Ok(write_queries)
}

fn map_reading(mut reading: CSIReading, frame_map: &Arc<DashMap<String, CSIStore>>) -> CSIReading {
    let sequence_identifier = reading.sequence_identifier;
    let key = format!("{}/{}", reading.mac.clone(), reading.antenna.clone());

    let WINDOW_SIZE: usize = config::get().lock().unwrap().buffer.window_size;

    match frame_map.get_mut(&key) {
        Some(mut stored_frame) => {
            // Get interval
            let stored_reading = &stored_frame.reading;

            let ret_sequence: i32 = stored_reading.sequence_identifier;
            let new_interval = sequence_identifier - ret_sequence;

            // check if this frame arrived out of sequence
            // if so, don't generate metrics as they won't mean anything.
            if sequence_identifier < ret_sequence {
                reading.interval = ret_sequence;
                // TODO: Add telemetry message to indicate this occurred.
            } else {
                // Get PCC
                let new_matrix = reading.csi_matrix.clone();
                let corr = csi::get_correlation_coefficient(new_matrix.clone(), &stored_reading.csi_matrix);

                reading.correlation_coefficient = corr;
                reading.interval = new_interval;

                if stored_frame.counter > WINDOW_SIZE {
                    // reset counter
                    stored_frame.counter = 0;

                    let first_frame: &CSIReading = stored_frame.buffer.peek().unwrap();
                    let mut prev_frame: &CSIReading = stored_frame.buffer.peek().unwrap();
                    let mut prim_vec = Vec::new();
                    for frame in stored_frame.buffer.iter() {
                        if frame.timestamp_us < prev_frame.timestamp_us {
                            // frame received out of order. drop this one.
                            continue;
                        } else if (frame.timestamp_us - first_frame.timestamp_us) > 1000000 { // if the window exceeds the time frame (1s in microseconds)
                            break;
                        } else {
                            prim_vec.push(frame.csi_matrix.clone());
                            prev_frame = frame;
                        }
                    }

                    // let mut matrix = Array::zeros((prim_vec.len(), csi::ACTIVE_SUBCARRIERS));
                    // for (i, frame) in prim_vec.iter().enumerate() {
                    //     for j in range(0, frame.len()) {
                    //         matrix[[i, j]] = frame[[0, j]];
                    //     }
                    // }
                    //
                    // print!("{:?}", matrix.shape());
                    // print!("first_frame: {} frame: {}\n", first_frame.timestamp_us, stored_frame.buffer.back().unwrap().timestamp_us);

                    // let resampled_sequence = sci_rs::signal::resample::resample(matrix.slice_axis(Axis(0), ), WINDOW_SIZE);

                    // compute metrics. for fun. and profit.
                    let corr_window = csi::get_correlation_coefficient(
                        prim_vec.first().unwrap().clone(),
                        &prim_vec.last().unwrap().clone()
                    );
                    // let corr_window = csi::get_correlation_coefficient(
                    //     stored_frame.buffer.peek().unwrap().csi_matrix.clone(),
                    //     &stored_frame.buffer.back().unwrap().csi_matrix.clone()
                    // );

                    reading.correlation_coefficient = corr_window;
                } else {
                    // print!("{}\n", stored_frame.buffer.len());
                    stored_frame.buffer.push(reading.clone());
                    stored_frame.counter += 1;

                    reading.correlation_coefficient = stored_frame.reading.correlation_coefficient;
                }
            }

            if reading.interval > 65000 {
                let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                reading.interval = sequence_identifier + ret_diff_from_max;
            }

            // *stored_frame = reading.clone();
            stored_frame.reading = reading.clone();
        }
        None => {
            frame_map.insert(key.clone(), CSIStore {
                buffer: AllocRingBuffer::new(WINDOW_SIZE),
                reading: reading.clone(),
                counter: 0
            });
            println!("Added new client with key: {} (time: {})", key.clone(), reading.time.clone());
        }
    }

    reading
}
