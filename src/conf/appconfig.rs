use config::{Config, ConfigError, Environment, File};
use serde_derive::Deserialize;
use std::env;

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct Message {
    address: String,
    buffer_size: i32,
    port: i16
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct CSI {
    frame_size: i16,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct Influx {
    protocol: String,
    address: String,
    port: i16,
    write_batch_size: i32,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct InfluxData {
    database: String,
    csi_metrics_measurement: String,
    sensor_telemetry_measurement: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct AppConfig {
    message: Message,
    csi: CSI,
    influx: Influx,
    influx_data: InfluxData,
}

impl AppConfig {
    pub fn new() -> Result<Self, ConfigError> {
        // let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            .add_source(File::with_name("src/conf/app.toml"))
            .build()?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}