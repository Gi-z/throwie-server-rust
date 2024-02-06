use std::collections::HashMap;

use influxdb::{Client, WriteQuery, InfluxDbWriteable};

use miniz_oxide::inflate::decompress_to_vec;

use byteorder::{ByteOrder, LittleEndian};

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

    let mut frame_map: HashMap<String, csi::CSIMessageReading> = HashMap::new();

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
        } else if recv_buf[0] == 0x57 {
            // batch of readings

            let potential_size = LittleEndian::read_u16(&recv_buf[ 2 .. 4]) as usize;
            let compressed_data = &recv_buf[ 5 .. (potential_size + 4) ];

            let decompressed_data = miniz_oxide::inflate::decompress_to_vec_with_limit(compressed_data, 60000).expect("Failed to decompress!");

            for i in 1 .. 16 {
                let protobuf_size = decompressed_data[(146 * i)] as usize;
                let parse_result = csi::parse_csi_message(&decompressed_data[ (146 * i) + 1 .. ((146 * i) + 1) + protobuf_size ]);
                let msg = match parse_result {
                    Ok(m) => m,
                    Err(_) => continue
                };

                let mut msg_reading: csi::CSIMessageReading = csi::get_reading(&msg);
                let mut reading = msg_reading.reading.clone();
                let sequence_identifier = reading.sequence_identifier;

                match frame_map.get(&reading.mac) {
                    Some(frame) => {
                        // Get interval
                        let ret_sequence: i32 = i32::try_from(frame.reading.sequence_identifier).ok().unwrap();
                        if ret_sequence > sequence_identifier {
                            // Wraparound has occurred. Get diff minus u16 max.
                            let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                            reading.interval = sequence_identifier + ret_diff_from_max;
                        } else {
                            reading.interval = sequence_identifier - ret_sequence;
                        }

                        // Get PCC
                        let new_matrix = msg_reading.csi_matrix.clone();
                        let corr = csi::get_correlation_coefficient(new_matrix, &frame.csi_matrix).unwrap();

                        reading.correlation_coefficient = corr;

                        *frame_map.get_mut(&reading.mac).unwrap() = msg_reading;
                    }
                    None => {
                        frame_map.insert(reading.mac.clone(), msg_reading);
                        println!("Added new client with src_mac: {} (time: {})", reading.mac.clone(), reading.time.clone());
                    }
                }

                write_queries.push(reading.into_query(csi::CSI_METRICS_MEASUREMENT));
            }
        } else {
            let parse_result = csi::parse_csi_message(&recv_buf[ 1 .. expected_size + 1]);
            let msg = match parse_result {
                Ok(m) => m,
                Err(_) => continue
            };

            let mut msg_reading: csi::CSIMessageReading = csi::get_reading(&msg);
            let mut reading = msg_reading.reading.clone();
            let sequence_identifier = reading.sequence_identifier;

            match frame_map.get(&reading.mac) {
                Some(frame) => {
                    // Get interval
                    let ret_sequence: i32 = i32::try_from(frame.reading.sequence_identifier).ok().unwrap();
                    if ret_sequence > sequence_identifier {
                        // Wraparound has occurred. Get diff minus u16 max.
                        let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
                        reading.interval = sequence_identifier + ret_diff_from_max;
                    } else {
                        reading.interval = sequence_identifier - ret_sequence;
                    }

                    // Get PCC
                    let new_matrix = msg_reading.csi_matrix.clone();
                    let corr = csi::get_correlation_coefficient(new_matrix, &frame.csi_matrix).unwrap();

                    reading.correlation_coefficient = corr;

                    *frame_map.get_mut(&reading.mac).unwrap() = msg_reading;
                }
                None => {
                    frame_map.insert(reading.mac.clone(), msg_reading);
                    println!("Added new client with src_mac: {} (time: {})", reading.mac.clone(), reading.time.clone());
                }
            }

            write_queries.push(reading.into_query(csi::CSI_METRICS_MEASUREMENT));
        }
        
        if write_queries.len() > MESSAGE_BATCH_SIZE {
            let batch = write_queries.clone();
            write_batch(&client, batch).await;
            write_queries.clear();
        }
    }
}