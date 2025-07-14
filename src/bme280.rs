use chrono::{DateTime, NaiveDateTime, Utc};
use eui48::MacAddress;
use prost::{DecodeError, Message};

use crate::throwie::Bme280Reading;

#[derive(Clone, Debug)]
pub struct BME280Entry {
    pub timestamp: NaiveDateTime,

    pub sensor_id: MacAddress,
    pub version: String,
    pub temperature: f32,
    pub humidity: f32,
    pub pressure: f32,
}

impl BME280Entry {
    pub fn new(msg: &Bme280Reading) -> Self{
        let timestamp_us = i64::try_from(msg.timestamp).unwrap();
        let time = DateTime::from_timestamp_micros(timestamp_us).unwrap().naive_utc();

        let temperature = msg.temperature;
        let humidity = msg.humidity;
        let pressure = msg.pressure;

        let version = msg.version.clone();

        println!("{:?} {:?}", msg, time);

        let mac_arr: [u8; 6] = msg.device_mac.clone().try_into().unwrap();
        let device_mac = MacAddress::new(mac_arr);

        Self {
            timestamp: time,
            sensor_id: device_mac,
            version,

            temperature,
            humidity,
            pressure

        }
    }
}

pub fn parse_protobuf(expected_protobuf: &[u8]) -> Result<Bme280Reading, DecodeError> {
    match Bme280Reading::decode(expected_protobuf) {
        Ok(t) => Ok(t),
        Err(e) => Err(e),
    }
}