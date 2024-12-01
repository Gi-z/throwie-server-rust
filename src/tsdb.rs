use futures::pin_mut;

use tokio_postgres::{Client, NoTls};
use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::{ToSql, Type};
use crate::config;
use crate::config::Timescale;
use crate::csi::CSIStorageEntry;
use crate::telemetry::TelemetryEntry;

pub struct TimescaleClient {
    client: Client
}

impl TimescaleClient {
    pub async fn new() -> Self{
        let config = &config::get().lock().unwrap().timescale;

        Self{
            client: Self::get_client(config).await
        }
    }

    pub async fn write_csi_data_batch(&mut self, given_batch: &[CSIStorageEntry]) {
        if given_batch.len() == 0 {
            return;
        }

        let sink = self.client
            .copy_in("COPY csi_data \
                (sensor_id, timestamp, imag, real, sequence_identifier, \
                antenna, rssi, noise_floor) FROM STDIN BINARY")
            .await.unwrap();

        let mut writer = BinaryCopyInWriter::new(sink,
            &[Type::VARCHAR, Type::TIMESTAMP, Type::BYTEA, Type::BYTEA, Type::INT4,
                    Type::INT2, Type::INT2, Type::INT2]);

        // Pin the writer since it will be used in async operations
        pin_mut!(writer);

        let mut row: Vec<&'_ (dyn ToSql + Sync)> = Vec::new();

        for entry in given_batch {
            row.clear();
            row.push(&entry.sensor_id);
            row.push(&entry.timestamp);
            row.push(&entry.imag);
            row.push(&entry.real);
            row.push(&entry.sequence_identifier);
            row.push(&entry.antenna);
            row.push(&entry.rssi);
            row.push(&entry.noise_floor);
            writer.as_mut().write(&row).await.unwrap();
        }

        match writer.finish().await {
            Ok(_) => println!("Write successful. Wrote {} csi_data rows.", given_batch.len()),
            Err(e) => panic!("{}", e),
        }
    }

    pub async fn write_telemetry_batch(&mut self, given_batch: &[TelemetryEntry]) {
        if given_batch.len() == 0 {
            return;
        }

        let sink = self.client
            .copy_in("COPY csi_telemetry \
                (sensor_id, timestamp, sequence_identifier, uptime_ms, \
                version, device_type, message_type, is_eth) FROM STDIN BINARY")
            .await.unwrap();

        let writer = BinaryCopyInWriter::new(sink,
           &[Type::TEXT, Type::TIMESTAMP, Type::INT4, Type::INT4, Type::TEXT, Type::INT2,
                 Type::INT2, Type::BOOL]);

        // Pin the writer since it will be used in async operations
        pin_mut!(writer);

        let mut row: Vec<&'_ (dyn ToSql + Sync)> = Vec::new();

        for entry in given_batch {
            row.clear();
            row.push(&entry.sensor_id);
            row.push(&entry.timestamp);
            row.push(&entry.current_sequence_identifier);
            row.push(&entry.uptime_ms);
            row.push(&entry.version);
            row.push(&entry.device_type);
            row.push(&entry.message_type);
            row.push(&entry.is_eth);
            writer.as_mut().write(&row).await.unwrap();
        }

        match writer.finish().await {
            Ok(_) => println!("Write successful. Wrote {} csi_telemetry rows.", given_batch.len()),
            Err(e) => panic!("{}", e),
        }
    }

    pub async fn get_client(config: &Timescale) -> Client {
        let (client, connection) = tokio_postgres::connect(
            &*config.url, NoTls).await.unwrap();

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        client
    }
}