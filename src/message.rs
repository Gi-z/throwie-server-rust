extern crate inflate;
extern crate num_enum;

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Instant, SystemTime};
use crate::{config, csi, telemetry};

use crate::error::RecvMessageError;

use influxdb::{WriteQuery, InfluxDbWriteable};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use crate::csi::CSIReading;
use crate::db::InfluxClient;

const UDP_MESSAGE_MAX_SIZE: usize = 2000;

fn timeit<F: Fn() -> T, T>(f: F) -> (T, u128) {
    let start = SystemTime::now();
    let result = f();
    let end = SystemTime::now();
    let duration = end.duration_since(start).unwrap();
    (result, duration.as_millis())
}

#[derive(IntoPrimitive, TryFromPrimitive, Debug)]
#[repr(u8)]
pub enum MessageType {
    Telemetry = 0x01,
    CSI = 0x02,
    CSICompressed = 0x03
}

#[derive(Debug)]
pub struct MessageData {
    format: MessageType,
    addr: SocketAddr,
    payload: Vec<u8>
}

pub(crate) struct MessageServer {
    db: InfluxClient,
    frame_map: HashMap<String, i32>,
    socket: UdpSocket,
}

impl MessageServer {

    pub fn new(db: InfluxClient) -> Self{
        let config = &config::get().lock().unwrap().message;
        let host = &config.address;
        let port = config.port.clone();

        let socket = Self::open_socket(host, port);
        let frame_map = HashMap::new();

        Self{
            db,
            frame_map,
            socket,
        }
    }

    fn csi_metrics_measurement() -> String {
        config::get().lock().unwrap().influx.csi_metrics_measurement.clone()
    }

    fn open_socket(host: &str, port: u16) -> UdpSocket {
        match UdpSocket::bind((String::from(host), port)) {
            Ok(sock) => {
                println!("Successfully bound port {}.", port);
                sock
            },
            Err(error) => panic!("Error binding port {:?}: {:?}", port, error)
        }
    }

    pub fn recv_buf(&self) -> Result<([u8; UDP_MESSAGE_MAX_SIZE], usize, SocketAddr), RecvMessageError> {
        let mut buf = [0; UDP_MESSAGE_MAX_SIZE];
        let (len, addr) = self.socket.recv_from(&mut buf)
            .expect("Didn't receive data");

        Ok((buf, len, addr))
    }

    pub fn recv_message(&self) -> Result<MessageData, RecvMessageError> {
        let (recv_buf, payload_size, addr) = self.recv_buf()?;

        let payload = recv_buf[ 1 .. payload_size ].to_vec();

        let Ok(format) = MessageType::try_from(recv_buf[0]) else {
            return Err(RecvMessageError::MessageFormatDecodeError(recv_buf[0], addr, payload_size))
        };

        Ok(MessageData {
            format,
            addr,
            payload
        })
    }

    // pub async fn get_message(&mut self) -> Result<(), RecvMessageError> {
    //     let message = self.recv_message()?;
    //     let handled_message = self.handle_message(message)?;
    //
    //     self.db.add_readings(handled_message).await;
    //
    //     Ok(())
    // }

    pub async fn get_message(&mut self) -> Result<(), RecvMessageError> {
        let recv_start = Instant::now();
        let recv_message = self.recv_message()?;
        let recv_time = recv_start.elapsed();

        let handle_start = Instant::now();
        let handled_message = self.handle_message(recv_message)?;
        let handle_time = handle_start.elapsed();

        println!("recv_time: {}us", recv_time.as_micros());
        println!("handle_time: {}us", handle_time.as_micros());

        self.db.add_readings(handled_message).await;

        Ok(())
    }

    fn handle_message(&mut self, m: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        match m.format {
            MessageType::Telemetry => self.handle_telemetry(m),
            MessageType::CSI => self.handle_csi(m),
            MessageType::CSICompressed => self.handle_compressed_csi(m)
        }
    }

    fn handle_telemetry(&self, message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        let reading = self.parse_telemetry(&message.payload)?;
        Ok(vec![reading.into_query(&config::get().lock().unwrap().influx.sensor_telemetry_measurement)])
    }

    fn parse_telemetry(&self, expected_payload: &[u8]) -> Result<telemetry::TelemetryReading, RecvMessageError> {
        let protobuf_parse_result = telemetry::parse_telemetry_protobuf(expected_payload)?;
        Ok(telemetry::get_reading(&protobuf_parse_result))
    }

    fn handle_csi(&mut self, message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        let reading = self.parse_csi(&message.payload)?;
        let mapped_reading = self.map_frame(reading);

        Ok(vec![mapped_reading.into_query(Self::csi_metrics_measurement())])
    }

    fn parse_csi(&self, expected_payload: &[u8]) -> Result<csi::CSIReading, RecvMessageError>  {
        let protobuf_parse_result = csi::parse_csi_protobuf(expected_payload)?;
        csi::get_reading(&protobuf_parse_result)
    }

    fn handle_compressed_csi(&mut self, message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
        let mut write_queries: Vec<WriteQuery> = Vec::new();

        let compressed_frame_size = (config::get().lock().unwrap().message.csi_frame_size + 1) as usize;

        // batch of readings
        let decompressed_data = inflate::inflate_bytes_zlib(&message.payload).unwrap();
        let frame_count = decompressed_data.len() / compressed_frame_size;

        if (decompressed_data.len() % compressed_frame_size) > 0 {
            println!("Could not determine the number of frames in compressed container from {:?} with size: {:?}.", message.addr, decompressed_data.len());
            return Err(RecvMessageError::MessageDecompressionError())
        }

        // println!("Frames in container: {:?} from {}", frame_count, message.addr);

        for i in 0 .. frame_count {
            let protobuf_size = decompressed_data[compressed_frame_size * i] as usize;

            let protobuf_start = (compressed_frame_size * i) + 1;
            let protobuf_end = protobuf_start + protobuf_size;
            let protobuf_contents = &decompressed_data[ protobuf_start .. protobuf_end ];

            let Ok(msg) = csi::parse_csi_protobuf(protobuf_contents) else {
                println!("Invalid frame in decompressed array.");
                continue
            };

            let Ok(mut reading) = csi::get_reading(&msg) else {
                println!("Invalid frame in decompressed array.");
                continue
            };

            let mapped_reading = self.map_frame(reading);

            write_queries.push(mapped_reading.into_query(Self::csi_metrics_measurement()));
        }

        Ok(write_queries)
    }

    fn map_frame(&mut self, mut reading: CSIReading) -> CSIReading {
        let sequence_identifier = reading.sequence_identifier;
        let key = format!("{}/{}", reading.mac.clone(), reading.antenna.clone());

        match self.frame_map.get(&key) {
            Some(stored_frame) => {
                // Get interval
                // let ret_sequence: i32 = i32::try_from(stored_frame).ok().unwrap();
                let ret_sequence: i32 = stored_frame.clone();
                // if ret_sequence > sequence_identifier {
                //     // Wraparound has occurred. Get diff minus u16 max.
                //     println!("wraparound check is fuck");
                //     let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                //     reading.interval = sequence_identifier + ret_diff_from_max;
                // } else {
                //     reading.interval = sequence_identifier - ret_sequence;
                // }

                // if ret_sequence > sequence_identifier {
                //     // Wraparound has occurred. Get diff minus u16 max.
                //     // println!("wraparound check is fuck");
                //     let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                //     reading.interval = sequence_identifier + ret_diff_from_max;
                //     println!("{:#?}", reading);
                // } else {
                reading.interval = sequence_identifier - ret_sequence;
                // }

                // Get PCC
                // let new_matrix = msg_reading.csi_matrix.clone();
                // let corr = csi::get_correlation_coefficient(new_matrix, &stored_frame.csi_matrix).unwrap();
                //
                // reading.correlation_coefficient = corr;

                *self.frame_map.get_mut(&key).unwrap() = sequence_identifier;
            }
            None => {
                self.frame_map.insert(key, sequence_identifier);
                println!("Added new client with src_mac: {} (time: {})", reading.mac.clone(), reading.time.clone());
            }
        }

        reading
    }

}