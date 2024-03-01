extern crate inflate;
extern crate num_enum;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use influxdb::WriteQuery;

use tokio::net::UdpSocket;
use tokio::sync::{Mutex, watch};
use tokio::time::sleep;

use crate::{config, handler};
use crate::db::{DbWatchConfig, InfluxClient, start_batch_watcher};
use crate::error::RecvMessageError;
use crate::handler::MessageHandler;

const UDP_MESSAGE_MAX_SIZE: usize = 2000;

#[derive(IntoPrimitive, TryFromPrimitive, Debug, PartialEq)]
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

pub async fn get_message(address: String, port: u16) -> Result<(), RecvMessageError> {
    let num_cpus = num_cpus::get();

    let db_tasks = 1;
    let handler_tasks = num_cpus -1;

    println!("Running MessageServer with {} db task and {} handler tasks.", db_tasks, handler_tasks);
    sleep(Duration::from_millis(1000)).await;

    // get batch write threshold from appconfig
    let batch_size = config::get().lock().unwrap().influx.write_batch_size as usize;

    // get reusable handles to mutex for db client and temp batch vector, and our frame map
    let batch: Arc<Mutex<Vec<WriteQuery>>> = Arc::new(Mutex::new(Vec::new()));
    let db = Arc::new(Mutex::new(InfluxClient::new()));
    let handler = Arc::new(MessageHandler::new());

    // create channel for receiving db write notification
    let (tx, mut rx) = watch::channel(false);
    let arc_tx = Arc::new(tx);

    // start thread to receive/handle db write batch limit notifications
    start_batch_watcher(DbWatchConfig{
        tx: arc_tx.clone(),
        rx,
        batch: batch.clone(),
        db: db.clone()
    });

    // num workers = num logical cpus
    for _ in 0..handler_tasks {
        // get local handles for tx and batch
        let batch = batch.clone();
        let tx = arc_tx.clone();
        let handler = handler.clone();

        // spawn worker thread
        tokio::spawn(async move {
            // different UdpSocket instance per worker
            // but the same connection is reused
            let socket = get_reusable_socket(String::from("0.0.0.0"), 6969);

            // continuously read next udp packet
            loop {
                // read incoming udp packet into max size buffer
                let mut recv_buf = [0; UDP_MESSAGE_MAX_SIZE];
                let (payload_size, addr) = socket.recv_from(&mut recv_buf)
                    .await
                    .expect("Didn't receive data");

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
                let handled_message = handler.handle_message(recv_message).unwrap();

                // lock the batch so we can add new writequeries
                // lock lasts until the handle is out of scope
                let mut local_batch_handle = batch.lock().await;
                local_batch_handle.extend(handled_message);

                // if the batch exceeds write threshold, send db write notification
                if local_batch_handle.len() > batch_size {
                    tx.send(true).unwrap();
                }
            }
        }).await.expect("TODO: panic message");
    }
    Ok(())
}