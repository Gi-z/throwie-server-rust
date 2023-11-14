use std::net::UdpSocket;

use ndarray_stats::CorrelationExt;
use num::Complex;
use num::integer::Roots;
use num::traits::float;
use thiserror::Error;

use num;

use influxdb::Timestamp;
use influxdb::InfluxDbWriteable;

use protobuf::Message;

include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));
use csimsg::CSIMessage;

pub const UDP_SERVER_PORT: u16 = 6969;
pub const UDP_MESSAGE_SIZE: usize = 170;

pub const CSI_METRICS_MEASUREMENT: &str = "csi_metrics";

#[derive(Error, Debug)]
pub enum RetrieveMsgError {
    #[error("Could not receive data from UDP socket.")]
    SocketRecvError(),

    #[error("CSI expected_size: {0} is larger than allocated buffer: {1}.")]
    MsgTooBigError(usize, usize),

    #[error("Failed to parse protobuf from buffer contents.")]
    ProtobufParseError(#[from] protobuf::Error),

    #[error("Failed to calculate PCC for given frames.")]
    PCCCalcError(),
}

#[derive(Clone)]
pub struct CSIMessageReading {
    pub reading: CSIReading,
    pub csi_matrix: Array<f32, Ix2>
}

#[derive(Clone, InfluxDbWriteable)]
pub struct CSIReading {
    time: Timestamp,
    rssi: i8,
    noise_floor: i8,
    pub correlation_coefficient: f32,
    pub sequence_identifier: i32,
    pub interval: i32,
    #[influxdb(tag)] pub mac: String,
}

pub fn open_csi_socket() -> UdpSocket {
    let socket_result = UdpSocket::bind(("0.0.0.0", UDP_SERVER_PORT));
    match socket_result {
        Ok(sock) => sock,
        Err(error) => panic!("Encountered error when opening port {:?}: {:?}", UDP_SERVER_PORT, error)
    }
}

pub fn recv_buf(socket: &UdpSocket) -> Result<([u8; 170], usize), RetrieveMsgError> {
    let mut buf = [0; UDP_MESSAGE_SIZE];
    let recv_result = socket.recv_from(&mut buf);
    let (_, _) = match recv_result {
        Ok(i) => i,
        Err(_) => return Err(RetrieveMsgError::SocketRecvError())
    };

    let expected_size = buf[0] as usize;
    
    // If the size we expect to read is too large for buf then throw error.
    if expected_size > UDP_MESSAGE_SIZE - 1 {
        return Err(RetrieveMsgError::MsgTooBigError(expected_size, UDP_MESSAGE_SIZE))
    }

    // let expected_buf = &buf[1 .. expected_size + 1];

    Ok((buf, expected_size))
}

pub fn parse_csi_message(expected_protobuf: &[u8]) -> Result<CSIMessage, protobuf::Error>  {
    CSIMessage::parse_from_bytes(expected_protobuf)
}

use ndarray::{Array, Ix2, Axis, concatenate};
use ndarray_stats;

fn get_csi_matrix(msg: &CSIMessage) -> Result<Array<f32, Ix2>, RetrieveMsgError> {
    let csi_data = msg.csi_data.clone().unwrap();
    let mut csi_matrix = Array::zeros((1, 64));
    for n in 1..64 {
        // print!("{:?}", csi_data);
        let imag = csi_data[n * 2] as i8 as f32;
        let real = csi_data[n * 2 + 1] as i8 as f32;

        let sum_of_squares = imag.powi(2) + real.powi(2);
        let norm = sum_of_squares.sqrt();

        csi_matrix[[0, n]] = norm;
    }

    // print!("{:?}", csi_matrix);

    Ok(csi_matrix)
}

pub fn get_correlation_coefficient(frame: Array<f32, Ix2>, frame2: &Array<f32, Ix2>) -> Result<f32, RetrieveMsgError> {
    let stacked = concatenate(Axis(0), &[frame2.view(), frame.view()]).unwrap();
    // print!("{:?}\n\n", stacked);

    let corr = stacked.pearson_correlation().unwrap();
    
    // print!("{:?}\n\n", corr);

    let act = corr[[1, 0]];

    Ok(act)
}

pub fn get_reading(msg: &CSIMessage) -> CSIMessageReading {
    let rssi = i8::try_from(msg.rssi.unwrap()).ok().unwrap();
    let noise_floor = i8::try_from(msg.noise_floor.unwrap()).ok().unwrap();

    let sequence_identifier: i32 = i32::try_from(msg.sequence_identifier.unwrap()).ok().unwrap();

    let timestamp_us = u128::try_from(msg.timestamp.unwrap()).unwrap();
    let timestamp = Timestamp::Microseconds(timestamp_us).into();

    let src_mac = format!("0x{:X}", msg.src_mac.clone().unwrap()[5]);

    let interval = 1;
    let correlation_coefficient = 0.0;
    
    let reading = CSIReading {
        time: timestamp,
        rssi: rssi,
        noise_floor: noise_floor,
        correlation_coefficient: correlation_coefficient,
        mac: src_mac,
        sequence_identifier: sequence_identifier,
        interval: interval
    };

    let csi_message_reading = CSIMessageReading {
        reading: reading,
        csi_matrix: get_csi_matrix(msg).unwrap()
    };

    csi_message_reading
}