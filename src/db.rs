extern crate influxdb;

use influxdb::{Client, WriteQuery};

// use crate::conf::appconfig::AppConfig;
const MESSAGE_BATCH_SIZE: usize = 1000;

pub(crate) struct InfluxClient {
    // config: AppConfig,
    client: Client,
    batch: Vec<WriteQuery>
}

impl InfluxClient {
    pub fn new() -> Self{
        // let url = &format!("{}://{}:{}",
        //   &config.influx.protocol,
        //   &config.influx.address,
        //   &config.influx.port
        // );
        // let database = &config.influx.database;
        let batch: Vec<WriteQuery> = Vec::new();

        Self{
            batch,
            // config,
            client: Self::get_client()
        }
    }

    pub async fn add_readings(&mut self, readings: Vec<WriteQuery>) {
        self.batch.extend(readings);

        if self.batch.len() > MESSAGE_BATCH_SIZE {
            self.write_batch().await;
            self.batch.clear();
        }
    }

    async fn write_batch(&self) {
        let write_result = self.client
            .query(&self.batch)
            .await;
        // println!("{}", write_result.unwrap());
        assert!(write_result.is_ok(), "Write result was not okay");
    }

    // pub fn get_client(url: &str, database: &str) -> Client {
    //     Client::new(url, database)
    // }
    pub fn get_client() -> Client {
        Client::new("http://csi-hub:8086", "influx")
    }
}