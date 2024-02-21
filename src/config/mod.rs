use std::fs;
use std::process::exit;
use serde_derive::Deserialize;

use toml;

const CONFIG_PATH: &str = "src/config/app.toml";

#[derive(Clone, Debug, Deserialize)]
#[allow(unused)]
pub struct Message {
    pub address: String,
    pub buffer_size: u32,
    pub csi_frame_size: i16,
    pub port: u16,
}

// #[derive(Clone, Debug, Deserialize)]
// #[allow(unused)]
// pub struct CSI {
//     pub frame_size: i16,
// }

#[derive(Clone, Debug, Deserialize)]
#[allow(unused)]
pub struct Influx {
    pub protocol: String,
    pub address: String,
    pub port: i16,
    pub write_batch_size: i32,

    pub database: String,
    pub csi_metrics_measurement: String,
    pub sensor_telemetry_measurement: String,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(unused)]
pub struct AppConfig {
    pub message: Message,
    // pub csi: CSI,
    pub influx: Influx,
}

impl AppConfig {
    pub fn new() -> Self{
        let contents = match fs::read_to_string(CONFIG_PATH) {
            Ok(c) => c,
            Err(_) => {
                eprintln!("Could not read config file: `{}`", CONFIG_PATH);
                exit(1);
            }
        };

        match toml::from_str(&contents) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Unable to parse config file: `{}`", CONFIG_PATH);
                exit(1);
            }
        }
    }
}