use std::error::Error;

mod csi;
mod config;
// mod config;
mod db;
mod error;
mod message;
mod telemetry;
mod throwie {
    include!(concat!(env!("OUT_DIR"), "/throwie.rs"));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>{
    let conf = config::AppConfig::new();

    // InfluxDB client.
    let client = db::InfluxClient::new(conf.influx);
    let mut server = message::MessageServer::new(conf.message, client);

    loop {
        server.get_message().await?;
    }
}