use crate::error::RecvMessageError;

mod csi;
mod config;
mod db;
mod error;
mod message;
mod telemetry;
mod handler;

mod throwie {
    include!(concat!(env!("OUT_DIR"), "/throwie.rs"));
}

#[tokio::main]
async fn main() -> Result<(), RecvMessageError> {
    message::get_message().await
}