use influxdb::{WriteQuery, InfluxDbWriteable};

use crate::{config, csi, telemetry};
use crate::error::RecvMessageError;
use crate::message::{MessageData, MessageType};
use crate::throwie::CsiMessage;

use std::sync::Arc;

use dashmap::DashMap;
use prost::DecodeError;
use crate::csi::CSIReading;

fn csi_metrics_measurement() -> String {
    config::get().lock().unwrap().influx.csi_metrics_measurement.clone()
}

pub struct MessageHandler {
    frame_map: Arc<DashMap<String, CSIReading>>
}

impl MessageHandler {
    pub fn new() -> Self{
        Self{
            frame_map: Arc::new(DashMap::new())
        }
    }

    pub fn handle_message(&mut self, m: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        match m.format {
            MessageType::Telemetry => self.handle_telemetry(m),
            MessageType::CSI => self.handle_csi(m),
            MessageType::CSICompressed => self.handle_compressed_csi(m)
        }
    }

    fn handle_telemetry(&self, message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        let reading = self.parse_telemetry(&message.payload)?;
        Ok(vec![reading.into_query(&config::get().lock().unwrap().influx.sensor_telemetry_measurement)])
    }

    fn parse_telemetry(&self, expected_payload: &[u8]) -> Result<telemetry::TelemetryReading, RecvMessageError> {
        let protobuf_parse_result = telemetry::parse_telemetry_protobuf(expected_payload)?;
        Ok(telemetry::get_reading(&protobuf_parse_result))
    }

    fn handle_csi(&mut self, message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        let frame = self.parse_csi(&message.payload)?;
        let mapped_reading = self.map_reading(frame);

        Ok(vec![mapped_reading.into_query(csi_metrics_measurement())])
    }

    fn parse_csi(&self, expected_payload: &[u8]) -> Result<CSIReading, RecvMessageError>  {
        let frame = csi::parse_csi_protobuf(expected_payload)?;
        csi::get_reading(&frame)
    }

    fn handle_compressed_csi(&mut self, message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        let mut write_queries: Vec<WriteQuery> = Vec::new();

        let compressed_frame_size = (config::get().lock().unwrap().message.csi_frame_size + 1) as usize;

        // batch of readings
        let decompressed_data = inflate::inflate_bytes_zlib(&message.payload).unwrap();
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

            let Ok(mut reading) = csi::get_reading(&msg) else {
                println!("Invalid frame in decompressed array.");
                continue
            };

            // write_queries.push(reading.into_query(csi_metrics_measurement()));

            let mapped_reading = self.map_reading(reading);
            write_queries.push(mapped_reading.into_query(csi_metrics_measurement()));
        }

        Ok(write_queries)
    }

    fn map_reading(&mut self, mut reading: CSIReading) -> CSIReading {
        let sequence_identifier = reading.sequence_identifier;
        let key = format!("{}/{}", reading.mac.clone(), reading.antenna.clone());

        match self.frame_map.get(&key) {
            Some(stored_frame) => {
                // Get PCC
                let new_matrix = reading.csi_matrix.clone();
                let corr = csi::get_correlation_coefficient(new_matrix, &stored_frame.csi_matrix);

                // Get interval
                let ret_sequence: i32 = stored_frame.sequence_identifier;

                reading.interval = sequence_identifier - ret_sequence;
                reading.correlation_coefficient = corr;

                *self.frame_map.get_mut(&key).unwrap() = reading.clone();
            }
            None => {
                self.frame_map.insert(key, reading.clone());
                println!("Added new client with src_mac: {} (time: {})", reading.mac.clone(), reading.time.clone());
            }
        }

        reading
    }

}