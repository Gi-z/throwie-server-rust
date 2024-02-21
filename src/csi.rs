use ndarray_stats::CorrelationExt;

use num;

use influxdb::Timestamp;
use influxdb::InfluxDbWriteable;

use crate::throwie::CsiMessage;

use ndarray::{Array, Ix2, Axis, concatenate};
use ndarray_stats;
use prost::{DecodeError, Message};
use crate::error::RecvMessageError;

const FILTER_SUBARRIERS: [u8; 13] = [0, 1, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37];
const REQUIRED_SUBCARRIERS: [usize; 51] = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63];

#[derive(Clone, InfluxDbWriteable, Debug)]
pub struct CSIReading {
    pub time: Timestamp,
    rssi: i8,
    noise_floor: i8,
    pub correlation_coefficient: f32,
    pub sequence_identifier: i32,
    pub interval: i32,
    #[influxdb(tag)] pub mac: String,
    #[influxdb(tag)] pub antenna: i8,
}

impl CSIReading {
    pub fn new(msg: &CsiMessage) -> Self{
        let timestamp_us = u128::try_from(msg.timestamp).unwrap();
        let time = Timestamp::Microseconds(timestamp_us).into();

        let antenna = i8::try_from(msg.antenna).unwrap();
        let rssi = i8::try_from(msg.rssi).unwrap();
        let noise_floor = i8::try_from(msg.noise_floor).unwrap();
        let sequence_identifier = i32::try_from(msg.sequence_identifier).unwrap();

        let mac = format!("0x{:X}", msg.src_mac.clone()[5]);

        let interval = 1;
        let correlation_coefficient = 0.0;

        Self {
            time,
            antenna,
            rssi,
            noise_floor,
            correlation_coefficient,
            mac,
            sequence_identifier,
            interval
        }
    }
}

pub fn parse_csi_protobuf(expected_protobuf: &[u8]) -> Result<CsiMessage, DecodeError>  {
    CsiMessage::decode(expected_protobuf)
}

fn get_csi_matrix(msg: &CsiMessage) -> Result<Array<f32, Ix2>, RecvMessageError> {
    let csi_data = msg.csi_data.clone();

    if csi_data.len() == 128 {
        let mut csi_matrix = Array::zeros((1, 64));

        for n in 1..64 {
            // print!("{:?}", csi_data);
            let imag = csi_data[n * 2] as i8 as f32;
            let real = csi_data[n * 2 + 1] as i8 as f32;

            let sum_of_squares = imag.powi(2) + real.powi(2);
            let norm = sum_of_squares.sqrt();

            // if norm == 0.0 {
            //     csi_matrix[[0, n]] = norm;
            // } else {
            //     let db_val = 20 as f32 * norm.log10();
            //     csi_matrix[[0, n]] = db_val;
            // }

            csi_matrix[[0, n]] = norm;
        }

        let mut filtered_csi_matrix = Array::zeros((1, 51));
        for (n, val) in REQUIRED_SUBCARRIERS.into_iter().enumerate() {
            filtered_csi_matrix[[0, n]] = csi_matrix[[0, val]];
        }

        Ok(filtered_csi_matrix)
    } else {
        let mut csi_matrix = Array::zeros((1, 53));

        for n in 1..53 {
            // print!("{:?}", csi_data);
            let imag = csi_data[n * 2] as i8 as f32;
            let real = csi_data[n * 2 + 1] as i8 as f32;

            let sum_of_squares = imag.powi(2) + real.powi(2);
            let norm = sum_of_squares.sqrt();

            // if norm == 0.0 {
            //     csi_matrix[[0, n]] = norm;
            // } else {
            //     let db_val = 20 as f32 * norm.log10();
            //     csi_matrix[[0, n]] = db_val;
            // }

            csi_matrix[[0, n]] = norm;
        }

        let mut filtered_csi_matrix = Array::zeros((1, 53));
        for n in 1..53 {
            filtered_csi_matrix[[0, n]] = csi_matrix[[0, n]];
        }

        Ok(filtered_csi_matrix)
    }
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