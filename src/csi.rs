extern crate eui48;
use eui48::MacAddress;

use crate::throwie::CsiMessage;

use chrono::{DateTime, NaiveDateTime};

// use ndarray::{Array, Ix2, Axis, concatenate};
// use ndarray_stats;
use prost::{DecodeError, Message};
use ringbuffer::AllocRingBuffer;

use crate::handler::{HandledMessage, InjectorReference};
use crate::error::RecvMessageError;

// const FILTER_SUBCARRIERS: [u8; 11] = [0, 1, 28, 29, 30, 31, 32, 33, 34, 35, 36];
// const REQUIRED_SUBCARRIERS: [usize; 53] = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63];
// // const REQUIRED_SUBCARRIERS: [usize; 60] = [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63];
//
pub const ACTIVE_SUBCARRIERS: usize = 53;
pub const TOTAL_SUBCARRIERS: usize = 64;

#[derive(Clone, Debug)]
pub struct CSIStorageEntry {
    pub sensor_id: MacAddress,
    pub timestamp: NaiveDateTime,

    pub imag: Vec<u8>,
    pub real: Vec<u8>,

    pub amplitude: Vec<f32>,
    pub phase: Vec<f32>,

    pub sequence_identifier: i32,
    pub antenna: i16,
    pub rssi: i16,
    pub noise_floor: i16,
    pub fft_gain: i16,
    pub agc_gain: i16,

    pub correlation_coefficient: f32,
    pub interval: i32,
}

pub struct CSIStore {
    pub reading: CSIStorageEntry,
    pub buffer: AllocRingBuffer<CSIStorageEntry>,
    pub counter: usize
}

impl CSIStorageEntry {
    pub fn new(msg: &CsiMessage, injector_reference: &InjectorReference) -> Self{
        let mac_arr: [u8; 6] = msg.src_mac.clone().try_into().unwrap();
        let mac = MacAddress::new(mac_arr);

        let sequence_identifier = msg.sequence_identifier;
        let time = NaiveDateTime::from_timestamp_micros(msg.timestamp).unwrap();

        let (imag, real) = get_raw_csi_components(msg);
        let (amplitude, phase) = get_csi_amplitude_phase(msg);

        let antenna = msg.antenna as i16;
        let rssi = msg.rssi as i16;
        let noise_floor = (msg.noise_floor as i8) as i16;
        let fft_gain = (msg.fft_gain as u8) as i16;
        let agc_gain = (msg.agc_gain as u8) as i16;

        let correlation_coefficient = -1.0;
        let interval = -1;

        Self{
            sensor_id: mac,
            timestamp: time,

            imag,
            real,

            amplitude,
            phase,

            antenna,
            rssi,
            noise_floor,
            sequence_identifier,
            fft_gain,
            agc_gain,

            correlation_coefficient,
            interval,
        }
    }
}

pub fn parse_csi_protobuf(expected_protobuf: &[u8]) -> Result<CsiMessage, DecodeError>  {
    match CsiMessage::decode(expected_protobuf) {
        Ok(t) => Ok(t),
        Err(e) => Err(e),
    }
}

fn get_raw_csi_components(msg: &CsiMessage) -> (Vec<u8>, Vec<u8>) {
    let mut imag_comps = Vec::with_capacity(TOTAL_SUBCARRIERS);
    let mut real_comps = Vec::with_capacity(TOTAL_SUBCARRIERS);

    for subcarrier in msg.csi_data.chunks_exact(2) {
        imag_comps.push(subcarrier[0]);
        real_comps.push(subcarrier[1]);
    }

    //print!("{:?}", csi_matrix);

    (imag_comps, real_comps)
}

fn get_csi_amplitude_phase(msg: &CsiMessage) -> (Vec<f32>, Vec<f32>) {
    let mut amplitude = Vec::with_capacity(TOTAL_SUBCARRIERS);
    let mut phase = Vec::with_capacity(TOTAL_SUBCARRIERS);

    for chunk in msg.csi_data.chunks_exact(2) {
        let imag = chunk[0] as i8 as f32;
        let real = chunk[1] as i8 as f32;

        // Calculate amplitude (norm) and phase (angle)
        let norm = (real.powi(2) + imag.powi(2)).sqrt();
        let angle = imag.atan2(real);

        amplitude.push(norm);
        phase.push(angle);
    }

    (amplitude, phase)
}

pub fn get_correlation_coefficient(x: Vec<f32>, y: &Vec<f32>) -> f32 {
    let mean_x = x.iter().copied().sum::<f32>() / x.len() as f32;
    let mean_y = y.iter().copied().sum::<f32>() / y.len() as f32;

    // Compute the numerator and the denominator
    let mut numerator = 0.0;
    let mut denominator_x = 0.0;
    let mut denominator_y = 0.0;

    for i in 0..x.len() {
        let x_dev = x[i] - mean_x;
        let y_dev = y[i] - mean_y;

        numerator += x_dev * y_dev;
        denominator_x += x_dev * x_dev;
        denominator_y += y_dev * y_dev;
    }

    // Calculate the correlation coefficient
    numerator / (denominator_x.sqrt() * denominator_y.sqrt())
}

// pub fn get_scaling_factor(mag_vals: &Array<f32, Ix2>, rssi: i32) -> f32 {
//     let rssi_pwr = 10_f32.powi(rssi / 10);
//     // println!("Scaling CSIMeasurement CSI with RSSI_pwr {:?}", rssi_pwr);
//     let vec_mag = mag_vals.iter().map(|x| x.powi(2)).sum::<f32>();
//     // println!("Scaling opwedqqqwd {:?}", mag_vals);
//     let norm_vec_mag = vec_mag / 64_f32;
//
//     rssi_pwr / norm_vec_mag
// }

pub fn get_storage_entry(msg: &CsiMessage, injector_timestamp: &InjectorReference) -> Result<HandledMessage, RecvMessageError> {
    Ok(HandledMessage::CSIStorage(CSIStorageEntry::new(msg, injector_timestamp)))
}
