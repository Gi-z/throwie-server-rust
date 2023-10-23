use influxdb::{Client, WriteQuery};

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
    let client = Client::new("http://localhost:8086", "influx");
    
    // Open CSI UDP port.
    let socket = csi::open_csi_socket();
    println!("Successfully bound port {}.", csi::UDP_SERVER_PORT);

    let mut unique_clients: Vec<String> = Vec::new();
    let mut write_queries = Vec::new();

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

            let reading = telemetry::get_write_query(&msg);
            write_queries.push(reading);
        } else {
            let parse_result = csi::parse_csi_message(&recv_buf[ 1 .. expected_size + 1]);
            let msg = match parse_result {
                Ok(m) => m,
                Err(_) => continue
            };
            let reading = csi::get_write_query(&msg);

            let src_mac = format!("0x{:X}", msg.src_mac.clone().unwrap()[5]);
            if !unique_clients.contains(&src_mac) {
                unique_clients.push(src_mac.clone());
                println!("Added new client with src_mac: {}", &src_mac);
            }

            write_queries.push(reading);
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