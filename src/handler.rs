use std::ops::Add;
use crate::{bme280, config, csi, csi_metrics, pir, telemetry};
use crate::error::RecvMessageError;
use crate::message::{MessageData, MessageType};

use std::sync::Arc;
use chrono::{NaiveDateTime, TimeDelta};
use dashmap::DashMap;
use ndarray::{Array2, Axis, s, stack};
use ringbuffer::{AllocRingBuffer, RingBuffer};
use crate::bme280::BME280Entry;
use crate::csi::{CSIStorageEntry, CSIStore};
use crate::telemetry::TelemetryEntry;
use crate::pir::PIREntry;

use staged_sg_filter::sav_gol_f32;
use crate::csi_metrics::CSIMetricsPCCEntry;

#[derive(Clone)]
pub enum HandledMessage {
    CSIStorage(CSIStorageEntry),
    CSIMetricsPCC(CSIMetricsPCCEntry),
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
        let mapped_reading = self.process_and_update_state(frame);

        Ok(mapped_reading)
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
        // println!("Compressed CSI container with expected_size: {} actual size: {}", expected_compressed_size, message.payload.len() - 2);

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

            let mapped_reading = self.process_and_update_state(reading);
            // write_queries.push(mapped_reading.into_query(csi_metrics_measurement()));
            write_queries.extend(mapped_reading);
        }

        Ok(write_queries)
    }

    fn process_and_update_state(&self, msg: HandledMessage) -> Vec<HandledMessage> {
        let mut entry = match msg {
            HandledMessage::CSIStorage(e) => e,
            _ => panic!("Invalid message type passed to CSIHandler"),
        };

        let mut result = vec![];
        let key = format!("{}/{}", entry.sensor_id, entry.antenna);
        let sequence_identifier = entry.sequence_identifier;

        if let Some(mut stored) = self.frame_map.get_mut(&key) {
            let prev_entry = &stored.reading;
            let new_interval = sequence_identifier - prev_entry.sequence_identifier;

            if sequence_identifier < prev_entry.sequence_identifier {
                entry.interval = prev_entry.sequence_identifier;
            } else {
                let corr = csi::get_correlation_coefficient(
                    entry.amplitude.clone(),
                    &prev_entry.amplitude
                );
                entry.correlation_coefficient = corr;
                entry.interval = new_interval;

                if stored.counter > self.window_size {
                    stored.counter = 0;

                    let prim_vec: Vec<_> = stored.buffer.iter()
                        .filter(|f| f.timestamp >= stored.buffer.peek().unwrap().timestamp)
                        .take_while(|f| (f.timestamp - stored.buffer.peek().unwrap().timestamp) <= TimeDelta::seconds(1))
                        .map(|f| f.amplitude.clone())
                        .collect();

                    let mut matrix = Array2::<f32>::zeros((prim_vec.len(), csi::TOTAL_SUBCARRIERS));
                    for (i, row) in prim_vec.iter().enumerate() {
                        for j in 0..row.len() {
                            matrix[[i, j]] = row[j];
                        }
                    }

                    if let Some(pccs) = csi_metrics::compute_pcc_from_buffer(&matrix, self.window_size, 5) {
                        for (i, pcc) in pccs.iter().enumerate() {
                            let timestamp = stored.buffer.front().unwrap().timestamp.clone();
                            // let offset_timestamp = timestamp.add(TimeDelta::milliseconds(100) * i as i32);
                            let offset_timestamp = timestamp.add(TimeDelta::milliseconds(200) * i as i32);

                            result.push(HandledMessage::CSIMetricsPCC(CSIMetricsPCCEntry {
                                sensor_id: entry.sensor_id.clone(),
                                timestamp: offset_timestamp,
                                level: 2, // 100ms
                                correlation_coefficient: pcc.clone(),
                            }));
                        }
                    }
                } else {
                    stored.buffer.enqueue(entry.clone());
                    stored.counter += 1;
                }
            }

            stored.reading = entry.clone();
        } else {
            self.frame_map.insert(key.clone(), CSIStore {
                buffer: AllocRingBuffer::new(self.window_size),
                reading: entry.clone(),
                counter: 0,
            });
            println!("Added new CSI client: {}", key);
        }

        result.push(HandledMessage::CSIStorage(entry));
        result
    }
}
