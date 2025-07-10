use chrono::{DateTime, NaiveDateTime, Utc};
use eui48::MacAddress;
use prost::{DecodeError, Message};

use crate::handler::{HandledMessage, TelemetryHandler};
use crate::throwie::TelemetryMessage;

#[derive(Clone)]
pub struct TelemetryEntry {
    pub timestamp: NaiveDateTime,
    pub current_sequence_identifier: i32,
    pub uptime_ms: i32,

    // pub sensor_id: MacAddress,
    pub sensor_id: String,
    pub version: String,
    pub device_type: i16,
    pub message_type: i16,
    pub is_eth: bool,
}

impl TelemetryEntry {
    pub fn new(msg: &TelemetryMessage) -> Self{
        let timestamp_us = i64::try_from(msg.timestamp).unwrap();
        let time = DateTime::from_timestamp_micros(timestamp_us).unwrap().naive_utc();

        let message_type = msg.message_type as i16;
        let current_sequence_identifier = msg.current_sequence_identifier;
        let uptime_ms = msg.uptime_ms as i32;

        // println!("{:?} {:?}", msg, time);

        // let mac_arr: [u8; 6] = msg.device_mac.clone().try_into().unwrap();
        // let device_mac = MacAddress::new(mac_arr);
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

pub fn parse_telemetry_protobuf(expected_protobuf: &[u8]) -> Result<TelemetryMessage, DecodeError> {
    match TelemetryMessage::decode(expected_protobuf) {
        Ok(t) => Ok(t),
        Err(e) => Err(e),
    }
}