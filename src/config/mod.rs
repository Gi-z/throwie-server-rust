use std::fs;
use std::process::exit;
use std::sync::{Mutex, OnceLock};
use serde_derive::Deserialize;

use toml;

const CONFIG_PATH: &str = "src/config/app.toml";

#[derive(Clone, Debug, Deserialize)]
#[allow(unused)]
pub struct Buffer {
    pub window_size: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(unused)]
pub struct Message {
    pub address: String,
    pub csi_frame_size: i16,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(unused)]
pub struct Timescale {
    pub url: String,
    pub batch_time: u64,
    pub write_batch_size: i32
}

#[derive(Clone, Debug, Deserialize)]
#[allow(unused)]
pub struct AppConfig {
    pub buffer: Buffer,
    pub message: Message,
    pub timescale: Timescale
}

pub fn build() -> AppConfig{
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

pub fn get() -> &'static Mutex<AppConfig> {
    static CONFIG: OnceLock<Mutex<AppConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| Mutex::new(build()))
}