use std::fs;
use std::process::exit;
use serde_derive::Deserialize;

use toml;

const CONFIG_PATH: &str = "src/conf/app.toml";

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

    fn get_influx() -> Influx{

    }
}