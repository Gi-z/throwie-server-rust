extern crate inflate;
extern crate num_enum;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use std::net::SocketAddr;
use std::sync::Arc;
use dashmap::DashMap;

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::{config, handler};
use crate::error::RecvMessageError;
use crate::handler::HandledMessage;
use crate::dbmanager::start_db_watcher;

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
    udp_sock.set_reuse_address(true).unwrap();
    udp_sock.set_nonblocking(true).unwrap();
    udp_sock.bind(&socket2::SockAddr::from(addr)).unwrap();
    let udp_sock: std::net::UdpSocket = udp_sock.into();
    udp_sock.try_into().unwrap()
}

pub async fn get_message() -> Result<(), RecvMessageError> {
    let num_cpus = num_cpus::get();
    let handler_tasks = num_cpus - 1;

    println!("Running MessageServer with {} handler tasks.", handler_tasks);

    // create channel for receiving batch append notification
    let (db_append_batch_tx, db_append_batch_rx) = mpsc::channel::<Vec<HandledMessage>>(100);
    // start db manager to handle incoming data writes.
    start_db_watcher(db_append_batch_rx).await;

    let arc_frame_map = Arc::new(DashMap::new());

    for _ in 0..handler_tasks {
        // get local handles for tx and batch
        let task_append_batch_tx = db_append_batch_tx.clone();
        let frame_map = arc_frame_map.clone();

        // spawn worker threads
        tokio::spawn(async move {
            // different UdpSocket instance per worker
            // but the same connection is reused
            let address = String::from(&config::get().lock().unwrap().message.address);
            let port = config::get().lock().unwrap().message.port;

            let socket = get_reusable_socket(address, port);

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
                let handled_vector = handler::handle_message(recv_message, &frame_map).unwrap();
                task_append_batch_tx.send(handled_vector).await.expect("Batch append channel destroyed.")
            }
        }).await.expect("TODO: panic message");
    }
    Ok(())
}