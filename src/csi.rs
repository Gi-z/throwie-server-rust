use std::collections::VecDeque;
use ndarray_stats::CorrelationExt;

use influxdb::Timestamp;
use influxdb::InfluxDbWriteable;

use crate::throwie::CsiMessage;

use ndarray::{Array, Ix2, Axis, concatenate};
use ndarray_stats;
use prost::{DecodeError, Message};
use ringbuffer::AllocRingBuffer;
use crate::error::RecvMessageError;

const FILTER_SUBCARRIERS: [u8; 11] = [0, 1, 28, 29, 30, 31, 32, 33, 34, 35, 36];
const REQUIRED_SUBCARRIERS: [usize; 53] = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63];
// const REQUIRED_SUBCARRIERS: [usize; 60] = [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63];

pub const ACTIVE_SUBCARRIERS: usize = 53;

#[derive(Clone, InfluxDbWriteable, Debug)]
pub struct CSIReading {
    pub time: Timestamp,
    rssi: i8,
    noise_floor: i32,
    pub correlation_coefficient: f32,
    pub sequence_identifier: i32,
    pub interval: i32,
    #[influxdb(tag)] pub mac: String,
    #[influxdb(tag)] pub antenna: i8,

    #[influxdb(ignore)] pub csi_matrix: Array<f32, Ix2>,
    #[influxdb(ignore)] pub timestamp_us: u128
}

pub struct CSIStore {
    pub reading: CSIReading,
    pub buffer: AllocRingBuffer<CSIReading>,
    pub counter: usize
}

impl CSIReading {
    pub fn new(msg: &CsiMessage) -> Self{
        let timestamp_us = u128::try_from(msg.timestamp).unwrap();
        let time = Timestamp::Microseconds(timestamp_us).into();

        let antenna = i8::try_from(msg.antenna).unwrap();
        let rssi = i8::try_from(msg.rssi).unwrap();
        let noise_floor = i32::try_from(msg.noise_floor as i8).unwrap();
        let sequence_identifier = i32::try_from(msg.sequence_identifier).unwrap();

        let mac = format!("{:X}{:X}{:X}", msg.src_mac.clone()[3], msg.src_mac.clone()[4], msg.src_mac.clone()[5]);

        let interval = 1;
        let correlation_coefficient = 0.0;

        let csi_matrix = get_csi_matrix(msg).unwrap();

        Self {
            time,
            antenna,
            rssi,
            noise_floor,
            correlation_coefficient,
            mac,
            sequence_identifier,
            interval,
            csi_matrix,
            timestamp_us
        }
    }
}

pub fn parse_csi_protobuf(expected_protobuf: &[u8]) -> Result<CsiMessage, DecodeError>  {
    match CsiMessage::decode(expected_protobuf) {
        Ok(t) => Ok(t),
        Err(e) => Err(e),
    }
}

fn get_csi_matrix(msg: &CsiMessage) -> Result<Array<f32, Ix2>, RecvMessageError> {
    let csi_data = msg.csi_data.clone();

    let mut csi_matrix = Array::zeros((1, ACTIVE_SUBCARRIERS));

    for (dest, src) in REQUIRED_SUBCARRIERS.into_iter().enumerate() {
        //print!("{:?}", csi_data);
        let imag = csi_data[src * 2] as i8 as f32;
        let real = csi_data[src * 2 + 1] as i8 as f32;

        let sum_of_squares = imag.powi(2) + real.powi(2);
        let norm = sum_of_squares.sqrt();

        if norm == 0.0 {
            csi_matrix[[0, dest]] = norm;
        } else {
            let db_val = 20 as f32 * norm.log10();
            csi_matrix[[0, dest]] = db_val;
        }
    }

    //print!("{:?}", csi_matrix);

    let mut filtered_csi_matrix = Array::zeros((1, ACTIVE_SUBCARRIERS));
    let scaling_factor: f32 = get_scaling_factor(&csi_matrix, msg.rssi.clone());

    for n in 1..ACTIVE_SUBCARRIERS {
        // filtered_csi_matrix[[0, n]] = csi_matrix[[0, n]];
        filtered_csi_matrix[[0, n]] = csi_matrix[[0, n]] * scaling_factor.sqrt();
    }

    Ok(filtered_csi_matrix)
}

pub fn get_correlation_coefficient(frame: Array<f32, Ix2>, frame2: &Array<f32, Ix2>) -> f32 {
    let stacked = concatenate(Axis(0), &[frame2.view(), frame.view()]).unwrap();
    let corr = stacked.pearson_correlation().unwrap();

    corr[[1, 0]]
}

pub fn get_scaling_factor(mag_vals: &Array<f32, Ix2>, rssi: i32) -> f32 {
    let rssi_pwr = 10_f32.powi(rssi / 10);
    // println!("Scaling CSIMeasurement CSI with RSSI_pwr {:?}", rssi_pwr);
    let vec_mag = mag_vals.iter().map(|x| x.powi(2)).sum::<f32>();
    // println!("Scaling opwedqqqwd {:?}", mag_vals);
    let norm_vec_mag = vec_mag / 64_f32;

    rssi_pwr / norm_vec_mag
}

pub fn get_reading(msg: &CsiMessage) -> Result<CSIReading, RecvMessageError> {
    Ok(CSIReading::new(msg))
}
