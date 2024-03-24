use influxdb::{WriteQuery, InfluxDbWriteable};

use crate::{config, csi, telemetry};
use crate::error::RecvMessageError;
use crate::message::{MessageData, MessageType};

use std::sync::Arc;

use dashmap::DashMap;
use crate::csi::CSIReading;

fn csi_metrics_measurement() -> String {
    config::get().lock().unwrap().influx.csi_metrics_measurement.clone()
}

pub struct MessageHandler {
    frame_map: Arc<DashMap<String, CSIReading>>
}


pub fn handle_message(m: MessageData, f: &Arc<DashMap<String, CSIReading>>) -> Result<Vec<WriteQuery>, RecvMessageError> {
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

fn handle_csi(message: MessageData, f: &Arc<DashMap<String, CSIReading>>) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let frame = parse_csi(&message.payload)?;
    let mapped_reading = map_reading(frame, f);

    Ok(vec![mapped_reading.into_query(csi_metrics_measurement())])
}

fn parse_csi(expected_payload: &[u8]) -> Result<CSIReading, RecvMessageError>  {
    let frame = csi::parse_csi_protobuf(&expected_payload)?;
    csi::get_reading(&frame)
}

fn handle_compressed_csi(message: MessageData, f: &Arc<DashMap<String, CSIReading>>) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let mut write_queries: Vec<WriteQuery> = Vec::new();

    let compressed_frame_size = (config::get().lock().unwrap().message.csi_frame_size + 1) as usize;

    // batch of readings
    let decompressed_data = inflate::inflate_bytes_zlib(&message.payload).unwrap();
    let frame_count = decompressed_data.len() / compressed_frame_size;

    // if (decompressed_data.len() % compressed_frame_size) > 0 {
    //     println!("Could not determine the number of frames in compressed container from {:?} with size: {:?}.", message.addr, decompressed_data.len());
    //     return Err(RecvMessageError::MessageDecompressionError())
    // }

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

fn map_reading(mut reading: CSIReading, frame_map: &Arc<DashMap<String, CSIReading>>) -> CSIReading {
    let sequence_identifier = reading.sequence_identifier;
    let key = format!("{}/{}", reading.mac.clone(), reading.antenna.clone());

    match frame_map.get_mut(&key) {
        Some(mut stored_frame) => {
            // Get interval
            let ret_sequence: i32 = stored_frame.value().sequence_identifier;
            let new_interval = sequence_identifier - ret_sequence;

            // check if this frame arrived out of sequence
            // if so, don't generate metrics as they won't mean anything.
            if sequence_identifier < ret_sequence {
                reading.interval = ret_sequence;
                // TODO: Add telemetry message to indicate this occurred.
            } else {
                // Get PCC
                let new_matrix = reading.csi_matrix.clone();
                let corr = csi::get_correlation_coefficient(new_matrix, &stored_frame.csi_matrix);

                reading.correlation_coefficient = corr;
                reading.interval = new_interval;
            }

            if reading.interval > 65000 {
                let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                reading.interval = sequence_identifier + ret_diff_from_max;
            }

            *stored_frame = reading.clone();
        }
        None => {
            frame_map.insert(key.clone(), reading.clone());
            println!("Added new client with key: {} (time: {})", key.clone(), reading.time.clone());
        }
    }

    reading
}
