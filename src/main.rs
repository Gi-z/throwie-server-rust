
use std::net::UdpSocket;

use influxdb::{Client, WriteQuery, Timestamp};
use influxdb::InfluxDbWriteable;

use protobuf::Message;

include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));
use csimsg::{CSIMessage};

const UDP_SERVER_PORT: u16 = 6969;
const UDP_MESSAGE_SIZE: usize = 170;

const MESSAGE_BATCH_SIZE: usize = 500;

fn recv_message(socket: &UdpSocket) -> std::io::Result<CSIMessage> {
    let mut buf = [0; UDP_MESSAGE_SIZE];
    let (bytes_count, _) = socket.recv_from(&mut buf)?;

    let complete_buf = &mut buf[..bytes_count];
    let size = complete_buf[0] as usize;

    let expected_protobuf = &complete_buf[1 .. size + 1];

    let msg = CSIMessage::parse_from_bytes(expected_protobuf).unwrap();

    return Ok(msg);
}

#[derive(InfluxDbWriteable)]
struct CSIReading {
    time: Timestamp,
    rssi: i8,
    #[influxdb(tag)] mac: String,
}

async fn write_batch(client: &Client, readings: &Vec<WriteQuery>) {
    let write_result = client
        .query(readings)
        .await;
    assert!(write_result.is_ok(), "Write result was not okay");
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    {   
        // InfluxDB client.
        let client = Client::new("http://localhost:8086", "influx");

        let socket_result = UdpSocket::bind(("0.0.0.0", UDP_SERVER_PORT));
        let socket = match socket_result {
            Ok(sock) => sock,
            Err(error) => panic!("Encountered error when opening port {:?}: {:?}", UDP_SERVER_PORT, error)
        };

        println!("Successfully bound port {UDP_SERVER_PORT}.");

        let mut readings = Vec::new();

        loop {
            let msg: CSIMessage = recv_message(&socket)?;

            let rssi = i8::try_from(msg.rssi.unwrap()).ok().unwrap();
            // println!("RSSI: {}", rssi);

            let timestamp_us = u128::try_from(msg.timestamp.unwrap()).unwrap();
            let timestamp = Timestamp::Microseconds(timestamp_us).into();

            let mac = msg.src_mac.unwrap();
            let src_mac = mac[1 .. ].iter().fold("00".to_string(), |str, i| format!("{}:{:X}", str, i).to_string());
            
            // let new_reading = vec!(
            //     CSIReading {
            //         time: timestamp,
            //         rssi: rssi,
            //         mac: src_mac,
            //     }.into_query("messages"),
            // );
            let new_reading = CSIReading {
                time: timestamp,
                rssi: rssi,
                mac: src_mac,
            }.into_query("messages");
            
            // write_msg(&client, new_reading).await;
            
            readings.push(new_reading);
            if (readings.len() > MESSAGE_BATCH_SIZE) {
                write_batch(&client, readings).await;
                readings.clear();
            }
        }
    }
}