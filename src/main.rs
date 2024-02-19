// extern crate protobuf;
extern crate influxdb;
extern crate byteorder;
extern crate num_enum;

extern crate inflate;

use std::{collections::HashMap};

use influxdb::{Client, WriteQuery, InfluxDbWriteable};

mod csi;
mod telemetry;
mod message;
mod error;

mod throwie {
    include!(concat!(env!("OUT_DIR"), "/throwie.rs"));
}

use crate::message::handle_message;

const MESSAGE_BATCH_SIZE: usize = 1000;

async fn write_batch(client: &Client, readings: Vec<WriteQuery>) {
    let write_result = client
        .query(readings)
        .await;
    // println!("{}", write_result.unwrap());
    assert!(write_result.is_ok(), "Write result was not okay");
}

#[tokio::main]
async fn main() {
    // InfluxDB client.
    let client = Client::new("http://csi-hub:8086", "influx");

    // Open CSI UDP port.
    let socket = message::open_socket();
    let mut write_queries = Vec::new();

    // let mut frame_map: HashMap<String, csi::CSIMessageReading> = HashMap::new();

    loop {
        let Ok(message) = message::recv_message(&socket) else {
            continue;
        };

        write_queries.extend(handle_message(message).unwrap());

        if write_queries.len() > MESSAGE_BATCH_SIZE {
            let batch = write_queries.clone();
            write_batch(&client, batch).await;
            write_queries.clear();
        }
    }
}