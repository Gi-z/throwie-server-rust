use std::net::UdpSocket;

use thiserror::Error;

use influxdb::{Timestamp, WriteQuery};
use influxdb::InfluxDbWriteable;

use protobuf::{Message, Error};

include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));
use csimsg::{CSIMessage};

pub const UDP_SERVER_PORT: u16 = 6969;
pub const UDP_MESSAGE_SIZE: usize = 170;

const CSI_METRICS_MEASUREMENT: &str = "csi_metrics";

#[derive(Error, Debug)]
pub enum RetrieveCSIError {
    #[error("Could not receive data from UDP socket.")]
    SocketRecvError,

    #[error("CSI expected_size: {0} is larger than allocated buffer: {1}.")]
    CSITooBigError(usize, usize),

    #[error("Failed to parse protobuf from buffer contents.")]
    ProtobufParseError(#[from] protobuf::Error),
}

#[derive(InfluxDbWriteable)]
struct CSIReading {
    time: Timestamp,
    rssi: i8,
    #[influxdb(tag)] mac: String,
}

pub fn open_csi_socket() -> UdpSocket {
    let socket_result = UdpSocket::bind(("0.0.0.0", UDP_SERVER_PORT));
    match socket_result {
        Ok(sock) => sock,
        Err(error) => panic!("Encountered error when opening port {:?}: {:?}", UDP_SERVER_PORT, error)
    }
}

pub fn recv_message(socket: &UdpSocket) -> Result<CSIMessage, RetrieveCSIError> {
    let mut buf = [0; UDP_MESSAGE_SIZE];
    let (bytes_count, _) = socket.recv_from(&mut buf)
                                        .expect("Received no data");

    let expected_size = buf[0] as usize;
    
    // If the size we expect to read is too large for buf then throw error.
    if expected_size > UDP_MESSAGE_SIZE - 1 {
        return Err(RetrieveCSIError::CSITooBigError(expected_size, UDP_MESSAGE_SIZE))
    }

    let expected_protobuf = &buf[1 .. expected_size + 1];
    let msg = CSIMessage::parse_from_bytes(expected_protobuf)?;

    Ok(msg)
}

pub fn get_write_query(msg: &CSIMessage) -> WriteQuery {
    let rssi = i8::try_from(msg.rssi.unwrap()).ok().unwrap();

    let timestamp_us = u128::try_from(msg.timestamp.unwrap()).unwrap();
    let timestamp = Timestamp::Microseconds(timestamp_us).into();

    let src_mac = format!("0x{:X}", msg.src_mac.clone().unwrap()[5]);
    
    CSIReading {
        time: timestamp,
        rssi: rssi,
        mac: src_mac,
    }.into_query(CSI_METRICS_MEASUREMENT)
}

fn get_csi_measurement(socket: &UdpSocket) -> Option<WriteQuery> {
    match recv_message(&socket) {
        Ok(m) => Some(get_write_query(&m)),
        Err(error) => None
    }
}