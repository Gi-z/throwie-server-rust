use std::error::Error;
use crate::error::RecvMessageError;

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
async fn main() -> Result<(), RecvMessageError> {
    // InfluxDB client.
    let client = db::InfluxClient::new();
    let mut server = message::MessageServer::new(client);

    server.get_message().await.expect("TODO: panic message");

    Ok(())
}