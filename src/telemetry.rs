use chrono::NaiveDateTime;
use prost::{DecodeError, Message};

use crate::handler::HandledMessage;
use crate::throwie::TelemetryMessage;

#[derive(Clone)]
pub struct TelemetryEntry {
    pub timestamp: NaiveDateTime,
    pub current_sequence_identifier: i32,
    pub uptime_ms: i32,

    pub sensor_id: String,
    pub version: String,
    pub device_type: i16,
    pub message_type: i16,
    pub is_eth: bool,
}

impl TelemetryEntry {
    pub fn new(msg: &TelemetryMessage) -> Self{
        let timestamp_us = i64::try_from(msg.timestamp).unwrap();
        let time = NaiveDateTime::from_timestamp_micros(timestamp_us).unwrap();

        let message_type = msg.message_type as i16;
        let current_sequence_identifier = msg.current_sequence_identifier;
        let uptime_ms = msg.uptime_ms as i32;

        let device_mac = format!("{:X}{:X}{:X}", msg.device_mac.clone()[3], msg.device_mac.clone()[4], msg.device_mac.clone()[5]);
        let version = msg.version.clone();
        let device_type = msg.device_type as i16;
        let is_eth = msg.is_eth;

        Self {
            timestamp: time,
            message_type,
            current_sequence_identifier,
            uptime_ms,
            sensor_id: device_mac,
            version,
            device_type,
            is_eth,
        }
    }
}

// impl TelemetryReading {
//     pub fn new(msg: &TelemetryMessage) -> Self{
//         let timestamp_us = u128::try_from(msg.timestamp).unwrap();
//         let time = Timestamp::Microseconds(timestamp_us).into();
//
//         let message_type = msg.message_type as i8;
//         let current_sequence_identifier = msg.current_sequence_identifier as i16;
//         let uptime_ms = msg.uptime_ms;
//
//         let device_mac = format!("{:X}{:X}{:X}", msg.device_mac.clone()[3], msg.device_mac.clone()[4], msg.device_mac.clone()[5]);
//         let version = msg.version.clone();
//         let device_type = msg.device_type as i8;
//         let is_eth = msg.is_eth;
//
//         Self {
//             time,
//             message_type,
//             current_sequence_identifier,
//             uptime_ms,
//             device_mac,
//             version,
//             device_type,
//             is_eth,
//         }
//     }
// }

pub fn parse_telemetry_protobuf(expected_protobuf: &[u8]) -> Result<TelemetryMessage, DecodeError> {
    match TelemetryMessage::decode(expected_protobuf) {
        Ok(t) => Ok(t),
        Err(e) => Err(e),
    }
}

// pub fn get_reading(msg: &TelemetryMessage) -> TelemetryReading {
//     TelemetryReading::new(msg)
// }

pub fn get_entry(msg: &TelemetryMessage) -> HandledMessage {
    HandledMessage::Telemetry(TelemetryEntry::new(msg))
}