extern crate protobuf;
extern crate influxdb;
extern crate byteorder;
extern crate num_enum;

extern crate inflate;

use std::collections::HashMap;

use influxdb::{Client, WriteQuery, InfluxDbWriteable};

use byteorder::{ByteOrder, LittleEndian};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use thiserror::Error;

mod csi;
mod telemetry;

const MESSAGE_BATCH_SIZE: usize = 1000;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum MessageType {
    Telemetry = 0x01,
    CSI = 0x02,
    CSICompressed = 0x03
}

#[derive(Error, Debug)]
pub enum MessageDecodeError {
    #[error("Could not determine type for incoming Message.")]
    MessageTypeDecodeError(),
}

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
        let (recv_buf, payload_size, addr) = match recv_result {
            Ok(m) => m,
            Err(_) => continue
        };

        let expected_payload = &recv_buf[ 1 .. payload_size ];
        let Ok(message_type) = MessageType::try_from(recv_buf[0]) else {
            println!("Could not determine type for incoming message from {:?} with size: {:?}.", addr, payload_size);
            continue;
        };

        match message_type {
            MessageType::Telemetry => {
                let parse_result = telemetry::parse_telemetry_message(expected_payload);
                let msg = match parse_result {
                    Ok(m) => m,
                    Err(_) => continue
                };

                let reading = telemetry::get_reading(&msg);
                write_queries.push(reading.into_query(telemetry::SENSOR_TELEMETRY_MEASUREMENT));
            },
            MessageType::CSI => {
                let parse_result = csi::parse_csi_message(expected_payload);
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
                        // let new_matrix = msg_reading.csi_matrix.clone();
                        // let corr = csi::get_correlation_coefficient(new_matrix, &frame.csi_matrix).unwrap();

                        // reading.correlation_coefficient = corr;
                        reading.correlation_coefficient = 0.0;

                        *frame_map.get_mut(&reading.mac).unwrap() = msg_reading;
                    }
                    None => {
                        frame_map.insert(reading.mac.clone(), msg_reading);
                        println!("Added new client with src_mac: {} (time: {})", reading.mac.clone(), reading.time.clone());
                    }
                }

                write_queries.push(reading.into_query(csi::CSI_METRICS_MEASUREMENT));
            },
            MessageType::CSICompressed => {
                // batch of readings
                let decompressed_data = inflate::inflate_bytes_zlib(&expected_payload).unwrap();
                let frame_count = decompressed_data.len() / csi::COMPRESSED_CSI_FRAME_SIZE;

                if (decompressed_data.len() % csi::COMPRESSED_CSI_FRAME_SIZE) > 0 {
                    println!("Could not determine the number of frames in compressed container from {:?} with size: {:?}.", addr, decompressed_data.len());
                    continue;
                }

                for i in 0 .. frame_count {
                    let protobuf_size = decompressed_data[csi::COMPRESSED_CSI_FRAME_SIZE * i] as usize;
                    
                    let protobuf_start = (csi::COMPRESSED_CSI_FRAME_SIZE * i) + 1;
                    let protobuf_end = protobuf_start + protobuf_size;
                    let protobuf_contents = &decompressed_data[ protobuf_start .. protobuf_end ];

                    let parse_result = csi::parse_csi_message(protobuf_contents);
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

                    // if reading.mac == "0x69" {
                    //     println!("{:#?}", reading);
                    // }

                    write_queries.push(reading.into_query(csi::CSI_METRICS_MEASUREMENT));
                }
            }
        }
        
        if write_queries.len() > MESSAGE_BATCH_SIZE {
            let batch = write_queries.clone();
            write_batch(&client, batch).await;
            write_queries.clear();
        }
    }
}