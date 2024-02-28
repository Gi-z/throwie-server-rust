extern crate inflate;
extern crate num_enum;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc};
use futures::lock::Mutex;
use std::time::{Duration, Instant, SystemTime};
use influxdb::WriteQuery;

use tokio::net::UdpSocket;
use tokio::time::sleep;

use crate::{config, db, handler};
use crate::csi::CSIReading;
use crate::db::InfluxClient;
use crate::error::RecvMessageError;

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
    pub format: MessageType,
    pub addr: SocketAddr,
    pub payload: Vec<u8>
}

pub(crate) struct MessageServer {
    // db: InfluxClient,
    frame_map: HashMap<String, i32>,
    // socket: UdpSocket,
    host: String,
    port: u16
}

fn get_reusable_socket(host: String, port: u16) -> UdpSocket {
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
    // udp_sock.set_reuse_port(true).unwrap();
    // udp_sock.set_cloexec(true).unwrap();
    udp_sock.set_reuse_address(true).unwrap();
    udp_sock.set_nonblocking(true).unwrap();
    udp_sock.bind(&socket2::SockAddr::from(addr)).unwrap();
    let udp_sock: std::net::UdpSocket = udp_sock.into();
    udp_sock.try_into().unwrap()
}

impl MessageServer {

    pub fn new() -> Self{
        let config = &config::get().lock().unwrap().message;
        let host = config.address.clone();
        let port = config.port.clone();

        let frame_map = HashMap::new();

        // let db = db::InfluxClient::new();

        Self{
            // db,
            frame_map,
            host,
            port
        }
    }

    pub async fn get_message(&mut self) -> Result<(), RecvMessageError> {
        let num_cpus = num_cpus::get();
        println!("Running MessageServer on {} tasks.", num_cpus);
        sleep(Duration::from_millis(1000)).await;

        // get batch write threshold from appconfig
        let batch_size = config::get().lock().unwrap().influx.write_batch_size;

        // get reusable handles to mutex for db client and temp batch vector
        let batch: Arc<Mutex<Vec<WriteQuery>>> = Arc::new(Mutex::new(Vec::new()));
        let db = Arc::new(Mutex::new(InfluxClient::new()));

        // num workers = num logical cpus
        for _ in 0..num_cpus {
            // get local handles for db and batch
            let batch = batch.clone();
            let db = db.clone();
            // spawn worker thread
            tokio::spawn(async move {
                // different UdpSocket instance per worker
                // but the same connection is reused
                let socket = get_reusable_socket(String::from("0.0.0.0"), 6969);

                // continuously read next udp packet
                loop {
                    // disabled recv_from timer
                    // let recv_start = Instant::now();

                    // read incoming udp packet into max size buffer
                    let mut recv_buf = [0; UDP_MESSAGE_MAX_SIZE];
                    let (payload_size, addr) = socket.recv_from(&mut recv_buf)
                        .await
                        .expect("Didn't receive data");

                    // let recv_time = recv_start.elapsed();
                    // println!("recv_time from addr ({}): {}us", addr, recv_time.as_micros());

                    // get packet format from first byte
                    let format = MessageType::try_from(recv_buf[0]).unwrap();
                    // rest of buffer = actual payload
                    let payload = recv_buf[1..payload_size].to_vec();

                    let recv_message = MessageData {
                        format,
                        addr,
                        payload
                    };

                    // send messagedata to format-specific handler
                    // returns a vector which may contain writequeries to send to db
                    let handled_message = handler::handle_message(recv_message).unwrap();
                    // println!("{:?}", handled_message);

                    // separate block so we can drop the lock
                    {
                        // lock the batch so we can add new writequeries
                        let mut local_batch_handle = batch.lock().await;
                        local_batch_handle.extend(handled_message);

                        // if the batch exceeds write threshold, start a db write
                        if local_batch_handle.len() > batch_size as usize {
                            let batch_copy = local_batch_handle.clone();
                            local_batch_handle.clear();

                            // drop the lock on the batch asap so other workers can use it.
                            drop(local_batch_handle);

                            // lock db client so we can issue the write
                            let db_handle = db.lock().await;
                            db_handle.write_given_batch(batch_copy).await;
                        }
                    }
                }
            }).await.expect("TODO: panic message");
        }
        Ok(())
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