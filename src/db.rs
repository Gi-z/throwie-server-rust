extern crate influxdb;

use influxdb::{Client, WriteQuery};
use std::sync::{Mutex, RwLock};
use crate::config;
use crate::config::Influx;

pub(crate) struct InfluxClient {
    batch: Vec<WriteQuery>,
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

        let batch: Vec<WriteQuery> = Vec::new();

        Self{
            batch,
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

    // pub fn add_readings(&mut self, readings: Vec<WriteQuery>) {
    //     self.batch.extend(readings);
    // }

    async fn write_batch(&self) {
        let write_result = self.client
            .query(&self.batch)
            .await;
        // println!("{}", write_result.unwrap());
        assert!(write_result.is_ok(), "Write result was not okay");
    }

    pub async fn write_given_batch(&self, given_batch: Vec<WriteQuery>) {
        let write_result = self.client
            .query(given_batch)
            .await;
        // println!("{}", write_result.unwrap());
        assert!(write_result.is_ok(), "Write result was not okay");
    }

    pub fn get_client(url: &str, database: &str) -> Client {
        Client::new(url, database)
    }
}