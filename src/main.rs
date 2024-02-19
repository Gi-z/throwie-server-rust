mod csi;
// mod conf;
mod db;
mod error;
mod message;
mod telemetry;
mod throwie {
    include!(concat!(env!("OUT_DIR"), "/throwie.rs"));
}

use crate::message::handle_message;

#[tokio::main]
async fn main() {
    // let app_config = conf::appconfig::AppConfig::new();

    // InfluxDB client.
    let mut client = db::InfluxClient::new();
    // Open CSI UDP port.
    let socket = message::open_socket();

    loop {
        let Ok(message) = message::recv_message(&socket) else {
            continue;
        };

        client.add_readings(handle_message(message).unwrap()).await;
    }
}