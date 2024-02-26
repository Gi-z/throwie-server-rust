use std::error::Error;

mod csi;
mod config;
mod db;
mod error;
mod message;
mod telemetry;
mod throwie {
    include!(concat!(env!("OUT_DIR"), "/throwie.rs"));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>{
    // InfluxDB client.
    let client = db::InfluxClient::new();
    let mut server = message::MessageServer::new(client);

    loop {
        server.get_message().await?
    }
}