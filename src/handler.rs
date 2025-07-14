use crate::{bme280, config, csi, pir, telemetry};
use crate::error::RecvMessageError;
use crate::message::{MessageData, MessageType};

use std::sync::Arc;
use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use dashmap::DashMap;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use toml::value::Time;
use crate::bme280::BME280Entry;
use crate::csi::{CSIStorageEntry, CSIStore};
use crate::telemetry::TelemetryEntry;
use crate::pir::PIREntry;
use crate::throwie::Bme280Reading;
use crate::throwie::PirReading;

#[derive(Clone)]
pub enum HandledMessage {
    CSIStorage(CSIStorageEntry),
    Telemetry(TelemetryEntry),
    BME280(BME280Entry),
    PIR(PIREntry)
}

pub struct InjectorReference {
    pub(crate) timestamp: NaiveDateTime,
    pub(crate) sequence_identifier: i32
}

pub type TimestampStore = DashMap<String, InjectorReference>;

pub struct CSIHandler {
    frame_map: Arc<DashMap<String, CSIStore>>,
    timestamp_store: Arc<TimestampStore>,
    window_size: usize,
}

pub struct BME280Handler {}
pub struct PIRHandler {}

pub struct TelemetryHandler {
    timestamp_store: Arc<TimestampStore>
}

pub struct MessageHandler {
    csi_handler: CSIHandler,
    telemetry_handler: TelemetryHandler,
    bme280_handler: BME280Handler,
    pir_handler: PIRHandler
}

impl MessageHandler {

    pub fn new(csi_handler: CSIHandler, telemetry_handler: TelemetryHandler) -> Self {
        let bme280_handler = BME280Handler::new();
        let pir_handler = PIRHandler::new();

        Self {
            csi_handler,
            telemetry_handler,
            bme280_handler,
            pir_handler
        }
    }
    pub fn handle_message(&self, m: MessageData) -> Result<Vec<HandledMessage>, RecvMessageError> {
        match m.format {
            MessageType::Telemetry => self.telemetry_handler.handle(m),
            MessageType::CSI => self.csi_handler.handle(m),
            MessageType::CSICompressed => self.csi_handler.handle_compressed(m),
            MessageType::BME280 => self.bme280_handler.handle(m),
            MessageType::PIR => self.pir_handler.handle(m)
        }
    }
}

impl BME280Handler {
    pub fn new() -> Self { Self { } }

    pub fn handle(&self, message: MessageData) -> Result<Vec<HandledMessage>, RecvMessageError> {
        let bme280_entry = BME280Entry::new(&bme280::parse_protobuf(&message.payload)?);

        // println!("bme280 reading: {:?}", bme280_entry);

        Ok(vec![HandledMessage::BME280(bme280_entry)])
    }
}

impl PIRHandler {
    pub fn new() -> Self { Self { } }

    pub fn handle(&self, message: MessageData) -> Result<Vec<HandledMessage>, RecvMessageError> {
        let pir_entry = PIREntry::new(&pir::parse_protobuf(&message.payload)?);

        // println!("pir reading: {:?}", pir_entry);

        Ok(vec![HandledMessage::PIR(pir_entry)])
    }
}

impl TelemetryHandler {

    pub fn new(timestamp_store: Arc<TimestampStore>) -> Self {
        Self { timestamp_store }
    }
    pub fn handle(&self, message: MessageData) -> Result<Vec<HandledMessage>, RecvMessageError> {
        let entry = TelemetryEntry::new(&telemetry::parse_telemetry_protobuf(&message.payload)?);

        // update timestamp store
        // let sensor_id = entry.sensor_id.clone(); // TODO: reenable
        let sensor_id = "main".to_lowercase();
        let new_injector_reference = InjectorReference{
            timestamp: entry.timestamp.clone(),
            sequence_identifier: entry.current_sequence_identifier.clone()
        };

        match self.timestamp_store.get_mut(&sensor_id) {
            Some(mut injector_reference) => *injector_reference = new_injector_reference,
            None => {
                self.timestamp_store.insert(sensor_id.clone(), new_injector_reference);
                println!("Added new Injector source with sensor_id: {} (time: {}, sequence_id: {})", sensor_id.clone(), entry.timestamp.clone(), entry.current_sequence_identifier.clone());
            }
        }

        Ok(vec![HandledMessage::Telemetry(entry)])
    }
}

impl CSIHandler {

    pub fn new(frame_map: Arc<DashMap<String, CSIStore>>, timestamp_store: Arc<TimestampStore>, window_size: usize) -> Self {
        Self {
            frame_map,
            timestamp_store,
            window_size
        }
    }
    fn handle(&self, message: MessageData) -> Result<Vec<HandledMessage>, RecvMessageError> {
        let frame = self.parse(&message.payload)?;
        let mapped_reading = self.map_reading(frame);

        // Ok(vec![mapped_reading.into_query(csi_metrics_measurement())])
        Ok(vec![mapped_reading])
    }

    fn parse(&self, expected_payload: &[u8]) -> Result<HandledMessage, RecvMessageError>  {
        let frame = csi::parse_csi_protobuf(&expected_payload)?;
        let guard = self.timestamp_store.get("main").unwrap();
        let injector_reference = guard.value().clone();
        csi::get_storage_entry(&frame, injector_reference)
    }

    fn handle_compressed(&self, message: MessageData) -> Result<Vec<HandledMessage>, RecvMessageError> {
        let mut write_queries: Vec<HandledMessage> = Vec::new();

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

        let binding = self.timestamp_store.get("main").unwrap();
        let injector_reference = binding.value();

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

            let Ok(reading) = csi::get_storage_entry(&msg, &injector_reference) else {
                println!("Invalid frame in decompressed array.");
                continue
            };

            let mapped_reading = self.map_reading(reading);
            // write_queries.push(mapped_reading.into_query(csi_metrics_measurement()));
            write_queries.push(mapped_reading);
        }

        Ok(write_queries)
    }

    fn map_reading(&self, msg: HandledMessage) -> HandledMessage {
        let mut entry: CSIStorageEntry;

        match msg {
            HandledMessage::CSIStorage(m) => entry = m,
            _ => {panic!("aaaa")}
        }

        let sequence_identifier = entry.sequence_identifier;
        let key = format!("{}/{}", entry.sensor_id.clone(), entry.antenna.clone());

        match self.frame_map.get_mut(&key) {
            Some(mut stored_frame) => {
                // Get interval
                let stored_reading = &stored_frame.reading;

                let ret_sequence = stored_reading.sequence_identifier;
                let new_interval = sequence_identifier - ret_sequence;

                // check if this frame arrived out of sequence
                // if so, don't generate metrics as they won't mean anything.
                if sequence_identifier < ret_sequence {
                    entry.interval = ret_sequence;
                    // TODO: Add telemetry message to indicate this occurred.
                } else {
                    // Get PCC
                    let new_matrix = entry.amplitude.clone();
                    let corr = csi::get_correlation_coefficient(new_matrix.clone(), &stored_reading.amplitude);

                    entry.correlation_coefficient = corr;
                    entry.interval = new_interval;

                    if stored_frame.counter > self.window_size {
                        // reset counter
                        stored_frame.counter = 0;

                        let first_frame: &CSIStorageEntry = stored_frame.buffer.peek().unwrap();
                        let mut prev_frame: &CSIStorageEntry = stored_frame.buffer.peek().unwrap();
                        let mut prim_vec = Vec::new();
                        for frame in stored_frame.buffer.iter() {
                            if frame.timestamp < prev_frame.timestamp {
                                // frame received out of order. drop this one.
                                continue;
                            } else if (frame.timestamp - first_frame.timestamp) > TimeDelta::microseconds(1000000) { // if the window exceeds the time frame (1s in microseconds)
                                break;
                            } else {
                                prim_vec.push(frame.amplitude.clone());
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
                        //let corr_window = csi::get_correlation_coefficient(
                        //    prim_vec.first().unwrap().clone(),
                        //    &prim_vec.last().unwrap().clone()
                        //);
                        let corr_window = csi::get_correlation_coefficient(
                            stored_frame.buffer.peek().unwrap().amplitude.clone(),
                            &stored_frame.buffer.back().unwrap().amplitude.clone()
                        );

                        entry.correlation_coefficient = corr_window;
                    } else {
                        // print!("{}\n", stored_frame.buffer.len());
                        stored_frame.buffer.push(entry.clone());
                        stored_frame.counter += 1;

                        entry.correlation_coefficient = stored_frame.reading.correlation_coefficient;
                    }
                }

                if entry.interval > 65000 {
                    let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                    entry.interval = sequence_identifier + ret_diff_from_max;
                }

                // println!("{:?}", entry);

                // *stored_frame = reading.clone();
                stored_frame.reading = entry.clone();
            }
            None => {
                self.frame_map.insert(key.clone(), CSIStore {
                    buffer: AllocRingBuffer::new(self.window_size),
                    reading: entry.clone(),
                    counter: 0
                });
                println!("Added new client with key: {} (time: {})", key.clone(), entry.timestamp.clone());
            }
        }

        HandledMessage::CSIStorage(entry)
    }
}
