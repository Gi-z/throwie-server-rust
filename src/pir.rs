use chrono::{DateTime, NaiveDateTime, Utc};
use eui48::MacAddress;
use prost::{DecodeError, Message};

use crate::throwie::PirReading;

#[derive(Clone, Debug)]
pub struct PIREntry {
    pub timestamp: NaiveDateTime,

    pub sensor_id: MacAddress,
    pub version: String,
    pub level: bool
}

impl PIREntry {
    pub fn new(msg: &PirReading) -> Self{
        let timestamp_us = i64::try_from(msg.timestamp).unwrap();
        let time = DateTime::from_timestamp_micros(timestamp_us).unwrap().naive_utc();

        let level = msg.level;

        let version = msg.version.clone();

        // println!("{:?} {:?}", msg, time);

        println!("{:?}", msg);

        let mac_arr: [u8; 6] = msg.device_mac.clone().try_into().unwrap();
        let device_mac = MacAddress::new(mac_arr);

        Self {
            timestamp: time,
            sensor_id: device_mac,

            version,

            level
        }
    }
}

pub fn parse_protobuf(expected_protobuf: &[u8]) -> Result<PirReading, DecodeError> {
    match PirReading::decode(expected_protobuf) {
        Ok(t) => Ok(t),
        Err(e) => Err(e),
    }
}