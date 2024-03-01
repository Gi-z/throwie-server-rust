extern crate influxdb;

use influxdb::{Client, WriteQuery};
use std::sync::{Arc, OnceLock};
use dashmap::DashMap;
use tokio::sync::Mutex;
use tokio::sync::watch::{Receiver, Sender};
use crate::config;
use crate::config::{AppConfig, build};
use crate::csi::CSIReading;

pub struct InfluxClient {
    client: Client,
}

impl InfluxClient {
    pub fn new() -> Self{
        let config = &config::get().lock().unwrap().influx;

        let url = &format!("{}://{}:{}",
          &config.protocol,
          &config.address,
          &config.port
        );
        let database = &String::from(&config.database);

        Self{
            client: Self::get_client(url, database)
        }
    }

    pub async fn add_readings(&mut self, readings: Vec<WriteQuery>) {
        self.batch.extend(readings);

        if self.batch.len() > config::get().lock().unwrap().influx.write_batch_size as usize {
            self.write_batch().await;
            self.batch.clear();
        }
    }

    pub async fn write_given_batch(&self, given_batch: Vec<WriteQuery>) {
        let write_result = self.client
            .query(given_batch)
            .await;
        if !write_result.is_ok() {
            println!("{}", write_result.unwrap());
        }
    }

    pub fn get_client(url: &str, database: &str) -> Client {
        Client::new(url, database)
    }
}

pub struct DbWatchConfig {
    pub tx: Arc<Sender<bool>>,
    pub rx: Receiver<bool>,
    pub batch: Arc<Mutex<Vec<WriteQuery>>>,
    pub db: Arc<Mutex<InfluxClient>>
}

pub fn start_batch_watcher(mut config: DbWatchConfig) -> () {
    tokio::spawn(async move {
        while config.rx.changed().await.is_ok() { // watched channel value changed
            if *config.rx.borrow() == true { // only run when hitting batch size limit
                // reset channel value
                config.tx.send(false).unwrap();

                // lock the batch and clone it
                let mut batch_ref = config.batch.lock().await;
                let batch_copy = batch_ref.clone();

                // empty the batch and manually drop the lock
                batch_ref.clear();
                drop(batch_ref);

                // lock db client so we can issue the write
                let db_handle = config.db.lock().await;
                db_handle.write_given_batch(batch_copy).await;
            }
        }
    });
}
