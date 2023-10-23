use thiserror::Error;

use influxdb::{Timestamp, WriteQuery};
use influxdb::InfluxDbWriteable;

use protobuf::Message;

include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));
use telemetrymsg::TelemetryMessage;

const SENSOR_TELEMETRY_MEASUREMENT: &str = "telemetry";

#[derive(InfluxDbWriteable)]
struct TelemetryReading {
    time: Timestamp,
    current_sequence_identifier: i16,
    uptime_ms: i64,

    #[influxdb(tag)] device_mac: String,
    #[influxdb(tag)] version: String,
    #[influxdb(tag)] device_type: i8,
    #[influxdb(tag)] message_type: i8,
    #[influxdb(tag)] is_eth: bool,
}

pub fn parse_telemetry_message(expected_protobuf: &[u8]) -> Result<TelemetryMessage, protobuf::Error>  {
    TelemetryMessage::parse_from_bytes(expected_protobuf)
}

pub fn get_write_query(msg: &TelemetryMessage) -> WriteQuery {
    let timestamp_us = u128::try_from(msg.timestamp.unwrap()).unwrap();
    let timestamp = Timestamp::Microseconds(timestamp_us).into();

    let message_type = msg.message_type.unwrap().enum_value().unwrap() as i8;
    let current_sequence_identifier = msg.current_sequence_identifier.unwrap() as i16;
    let uptime_ms = msg.uptime_ms.unwrap();

    let device_mac = format!("0x{:X}", msg.device_mac.clone().unwrap()[5]);
    let version = msg.version.clone().unwrap();
    let device_type = msg.device_type.unwrap().enum_value().unwrap() as i8;
    let is_eth = msg.is_eth.unwrap();

    println!("Received telemetry message type: {} from device: {} and device_type: {} with uptime: {}.", message_type, device_mac, device_type, uptime_ms);
    // println!("Received telemetry message type: {} and device_type: {}.", message_type, device_type);

    TelemetryReading {
        time: timestamp,
        message_type: message_type,
        current_sequence_identifier: current_sequence_identifier,
        uptime_ms: uptime_ms,

        device_mac: device_mac,
        version: version,
        device_type: device_type,
        is_eth: is_eth
    }.into_query(SENSOR_TELEMETRY_MEASUREMENT)
}