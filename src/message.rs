extern crate inflate;
extern crate num_enum;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Instant, SystemTime};
use crate::{config, csi, telemetry};

use crate::error::RecvMessageError;

use influxdb::{WriteQuery, InfluxDbWriteable};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use crate::csi::CSIReading;
use crate::db::InfluxClient;

use tokio::net::UdpSocket;

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
    // socket: UdpSocket,
    host: String,
    port: u16
}

fn open_reusable_socket(host: String, port: u16) -> UdpSocket {
    let addr: SocketAddr = format!("{}:{}", host, port).parse().unwrap();
    let udp_sock = socket2::Socket::new(
        if addr.is_ipv4() {
            socket2::Domain::IPV4
        } else {
            socket2::Domain::IPV6
        },
        socket2::Type::DGRAM,
        None,
    ).unwrap();
    udp_sock.set_reuse_port(true).unwrap();
    udp_sock.set_cloexec(true).unwrap();
    udp_sock.set_nonblocking(true).unwrap();
    udp_sock.bind(&socket2::SockAddr::from(addr)).unwrap();
    let udp_sock: std::net::UdpSocket = udp_sock.into();
    udp_sock.try_into().unwrap()
}

impl MessageServer {

    pub fn new(db: InfluxClient) -> Self{
        let config = &config::get().lock().unwrap().message;
        let host = config.address.clone();
        let port = config.port.clone();

        // let socket = Self::open_reusable_socket(host, port);
        let frame_map = HashMap::new();

        Self{
            db,
            frame_map,
            // socket,
            host,
            port
        }
    }

    fn csi_metrics_measurement() -> String {
        config::get().lock().unwrap().influx.csi_metrics_measurement.clone()
    }

    // pub fn recv_buf(&self) -> Result<([u8; UDP_MESSAGE_MAX_SIZE], usize, SocketAddr), RecvMessageError> {
    //     let mut buf = [0; UDP_MESSAGE_MAX_SIZE];
    //     let (len, addr) = self.socket.recv_from(&mut buf)
    //         .expect("Didn't receive data");
    //
    //     Ok((buf, len, addr))
    // }

    // pub async fn get_message(&mut self) -> Result<(), RecvMessageError> {
    //     let message = self.recv_message()?;
    //     let handled_message = self.handle_message(message)?;
    //
    //     self.db.add_readings(handled_message).await;
    //
    //     Ok(())
    // }

    pub async fn get_message(&mut self) -> Result<(), RecvMessageError> {
        let num_cpus = num_cpus::get();

        let host = self.host.clone();
        let port = self.port.clone();

        for _ in 0..num_cpus {
            tokio::spawn(async move {
                let recv_start = Instant::now();

                let socket = open_reusable_socket(String::from("0.0.0.0"), port.clone());
                let mut recv_buf = [0; UDP_MESSAGE_MAX_SIZE];
                let (payload_size, addr) = socket.recv_from(&mut recv_buf)
                    .await
                    .expect("Didn't receive data");


                let payload = recv_buf[ 1 .. payload_size ].to_vec();

                let Ok(format) = MessageType::try_from(recv_buf[0]) else {
                    return Err(RecvMessageError::MessageFormatDecodeError(recv_buf[0], addr, payload_size))
                };

                let recv_message = MessageData {
                    format,
                    addr,
                    payload
                };

                let recv_time = recv_start.elapsed();
                println!("recv_time: {}us", recv_time.as_micros());

                Ok(())
            });
        }
        // let recv_start = Instant::now();
        // let recv_message = self.recv_message()?;
        // let recv_time = recv_start.elapsed();
        //
        // let handle_start = Instant::now();
        // let handled_message = self.handle_message(recv_message)?;
        // let handle_time = handle_start.elapsed();
        //
        // println!("recv_time: {}us", recv_time.as_micros());
        // println!("handle_time: {}us", handle_time.as_micros());
        //
        // self.db.add_readings(handled_message).await;
        //
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