use influxdb::Timestamp;
use influxdb::InfluxDbWriteable;
use prost::{DecodeError, Message};

use crate::throwie::{CsiMessage, TelemetryMessage};

#[derive(InfluxDbWriteable)]
pub struct TelemetryReading {
    time: Timestamp,
    current_sequence_identifier: i16,
    uptime_ms: i64,

    #[influxdb(tag)] device_mac: String,
    #[influxdb(tag)] version: String,
    #[influxdb(tag)] device_type: i8,
    #[influxdb(tag)] message_type: i8,
    #[influxdb(tag)] is_eth: bool,
}

impl TelemetryReading {
    pub fn new(msg: &TelemetryMessage) -> Self{
        let timestamp_us = u128::try_from(msg.timestamp).unwrap();
        let time = Timestamp::Microseconds(timestamp_us).into();

        let message_type = msg.message_type as i8;
        let current_sequence_identifier = msg.current_sequence_identifier as i16;
        let uptime_ms = msg.uptime_ms;

        let device_mac = format!("0x{:X}", &msg.device_mac[5]);
        let version = msg.version.clone();
        let device_type = msg.device_type as i8;
        let is_eth = msg.is_eth;

        Self {
            time,
            message_type,
            current_sequence_identifier,
            uptime_ms,
            device_mac,
            version,
            device_type,
            is_eth,
        }
    }
}

pub fn parse_telemetry_protobuf(expected_protobuf: &[u8]) -> Result<TelemetryMessage, DecodeError> {
    match TelemetryMessage::decode(expected_protobuf) {
        Ok(T) => Ok(T),
        Err(e) => Err(e),
    }
}

pub fn get_reading(msg: &TelemetryMessage) -> TelemetryReading {
    TelemetryReading::new(msg)
}