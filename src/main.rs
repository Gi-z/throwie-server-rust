use std::collections::HashMap;

use influxdb::{Client, WriteQuery, InfluxDbWriteable};

mod csi;
mod telemetry;

const MESSAGE_BATCH_SIZE: usize = 1000;

async fn write_batch(client: &Client, readings: Vec<WriteQuery>) {
    let write_result = client
        .query(readings)
        .await;
    // println!("{}", write_result.unwrap());
    assert!(write_result.is_ok(), "Write result was not okay");
}

#[tokio::main]
async fn main() {
    // InfluxDB client.
    let client = Client::new("http://csi-hub:8086", "influx");
    
    // Open CSI UDP port.
    let socket = csi::open_csi_socket();
    println!("Successfully bound port {}.", csi::UDP_SERVER_PORT);
    let mut write_queries = Vec::new();

    let mut SEQUENCE_MAP: HashMap<String, i32> = HashMap::new();

    loop {
        let recv_result = csi::recv_buf(&socket);
        let (recv_buf, expected_size) = match recv_result {
            Ok(m) => m,
            Err(_) => continue
        };

        if recv_buf[1] == 0xFF {
            let actual_protobuf = &recv_buf[ 2 .. (expected_size + 2) ];
            let parse_result = telemetry::parse_telemetry_message(actual_protobuf);
            let msg = match parse_result {
                Ok(m) => m,
                Err(_) => continue
            };

            let reading = telemetry::get_reading(&msg);
            write_queries.push(reading.into_query(telemetry::SENSOR_TELEMETRY_MEASUREMENT));
        } else {
            let parse_result = csi::parse_csi_message(&recv_buf[ 1 .. expected_size + 1]);
            let msg = match parse_result {
                Ok(m) => m,
                Err(_) => continue
            };

            let mut reading = csi::get_reading(&msg);
            let sequence_identifier = reading.sequence_identifier;

            match SEQUENCE_MAP.get(&reading.mac) {
                Some(ret_sequence) => {
                    if ret_sequence > &sequence_identifier {
                        // Wraparound has occurred. Get diff minus u16 max.
                        let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                        reading.interval = sequence_identifier + ret_diff_from_max;
                    } else {
                        reading.interval = sequence_identifier - ret_sequence;
                    }

                    *SEQUENCE_MAP.get_mut(&reading.mac).unwrap() = sequence_identifier;
                }
                None => {
                    SEQUENCE_MAP.insert(reading.mac.clone(), sequence_identifier);
                    println!("Added new client with src_mac: {}", reading.mac.clone());
                }
            }

            // let src_mac = format!("0x{:X}", msg.src_mac.clone().unwrap()[5]);
            // if !unique_clients.contains(&src_mac) {
            //     unique_clients.push(src_mac.clone());
            //     
            // }

            write_queries.push(reading.into_query(csi::CSI_METRICS_MEASUREMENT));
        }
        
        if write_queries.len() > MESSAGE_BATCH_SIZE {
            let batch = write_queries.clone();
            write_batch(&client, batch).await;
            // let client_task = client.clone();
            // tokio::spawn(async move {
            //     write_batch(client_task, batch).await;
            // });
            write_queries.clear();
        }
    }
}