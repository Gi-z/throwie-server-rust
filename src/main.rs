extern crate protobuf;
extern crate influxdb;
extern crate byteorder;
extern crate num_enum;

extern crate inflate;
extern crate config;

// use config::Config;
//
// use std::{collections::HashMap};
//
// use influxdb::{Client, WriteQuery, InfluxDbWriteable};
//
// mod csi;
// mod telemetry;
// mod message;
// mod error;
// mod db;
mod conf;

use conf::appconfig::AppConfig;

// const MESSAGE_BATCH_SIZE: usize = 1000;





#[tokio::main]
async fn main() {
    let app_config = AppConfig::new().unwrap();

    println!("{:?}", app_config);

    // InfluxDB client.
    let protocol = app_config.
    let client_url = format!("{}://{}:{}", );
    // let client = Client::new(, "influx");
    //
    //
    // // Open CSI UDP port.
    // let socket = message::open_socket();
    // let mut write_queries = Vec::new();
    //
    // let mut frame_map: HashMap<String, csi::CSIMessageReading> = HashMap::new();
    //
    // loop {
    //     let Ok(message) = message::recv(&socket) else {
    //         continue;
    //     };
    //
    //     write_queries.push(message::handle(message).unwrap());
    //
    //     if write_queries.len() > MESSAGE_BATCH_SIZE {
    //         let batch = write_queries.clone();
    //         write_batch(&client, batch).await;
    //         write_queries.clear();
    //     }
    // }
}