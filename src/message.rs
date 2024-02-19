use std::net::{SocketAddr, UdpSocket};
use crate::{csi, telemetry};

use crate::error::RecvMessageError;

use influxdb::{WriteQuery, InfluxDbWriteable};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use prost::DecodeError;
use crate::telemetry::TelemetryReading;

pub const UDP_HOST_ADDR: &str = "0.0.0.0";
pub const UDP_MESSAGE_SIZE: usize = 2000;
pub const UDP_SERVER_PORT: u16 = 6969;

#[derive(IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum MessageType {
    Telemetry = 0x01,
    CSI = 0x02,
    CSICompressed = 0x03
}
pub struct MessageData {
    format: MessageType,
    addr: SocketAddr,
    payload: &'static[u8]
}

pub fn open_socket() -> UdpSocket {
    match UdpSocket::bind((UDP_HOST_ADDR, UDP_SERVER_PORT)) {
        Ok(sock) => {
            println!("Successfully bound port {}.", UDP_SERVER_PORT);
            sock
        },
        Err(error) => panic!("Error binding port {:?}: {:?}", UDP_SERVER_PORT, error)
    }
}

pub fn recv_buf(socket: &UdpSocket) -> Result<([u8; UDP_MESSAGE_SIZE], usize, std::net::SocketAddr), RecvMessageError> {
    let mut buf = [0; UDP_MESSAGE_SIZE];
    let (len, addr) = socket.recv_from(&mut buf)
        .expect("Didn't receive data");

    Ok((buf, len, addr))
}

pub fn recv_message(socket: &UdpSocket) -> Result<MessageData, RecvMessageError> {
    let (recv_buf, payload_size, addr) = recv_buf(&socket)?;

    let payload = recv_buf[ 1 .. payload_size ];

    let Ok(format) = MessageType::try_from(recv_buf[0]) else {
        return Err(RecvMessageError::MessageFormatDecodeError(recv_buf[0], addr, payload_size))
    };

    Ok(MessageData {
        format,
        addr,
        payload
    })
}

pub fn handle_message(m: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    match m.format {
        MessageType::Telemetry => handle_telemetry(m),
        MessageType::CSI => handle_csi(m),
        MessageType::CSICompressed => handle_compressed_csi(m)
    }
}

pub fn handle_telemetry(message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let reading = parse_telemetry(&message.payload)?;
    Ok(vec![reading.into_query(telemetry::SENSOR_TELEMETRY_MEASUREMENT)])
} 

fn parse_telemetry(expected_payload: &[u8]) -> Result<TelemetryReading, RecvMessageError> {
    let protobuf_parse_result = telemetry::parse_telemetry_protobuf(expected_payload)?;
    Ok(telemetry::get_reading(&protobuf_parse_result))
}

pub fn handle_csi(message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    // println!("fuck");
    let mut reading = parse_csi(&message.payload)?;
    // let mut reading = msg_reading.reading.clone();
    let sequence_identifier = reading.sequence_identifier;

    let key = format!("{}/{}", reading.mac.clone(), reading.antenna.clone());

    // match frame_map.get(&key) {
    //     Some(stored_frame) => {
    //         // Get interval
    //         let ret_sequence: i32 = i32::try_from(stored_frame.reading.sequence_identifier).ok().unwrap();
    //         if ret_sequence > sequence_identifier {
    //             // Wraparound has occurred. Get diff minus u16 max.
    //             let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
    //             reading.interval = sequence_identifier + ret_diff_from_max;
    //         } else {
    //             reading.interval = sequence_identifier - ret_sequence;
    //         }
    //
    //         // Get PCC
    //         let new_matrix = msg_reading.csi_matrix.clone();
    //         let corr = csi::get_correlation_coefficient(new_matrix, &stored_frame.csi_matrix).unwrap();
    //
    //         reading.correlation_coefficient = corr;
    //
    //         *frame_map.get_mut(&key).unwrap() = msg_reading;
    //     }
    //     None => {
    //         frame_map.insert(key, msg_reading);
    //         println!("Added new client with addr: {} src_mac: {} (time: {})", addr, reading.mac.clone(), reading.time.clone());
    //     }
    // }

    Ok(vec![reading.into_query(csi::CSI_METRICS_MEASUREMENT)])
}

fn parse_csi(expected_payload: &[u8]) -> Result<csi::CSIReading, RecvMessageError>  {
    let protobuf_parse_result = csi::parse_csi_protobuf(expected_payload)?;
    csi::get_reading(&protobuf_parse_result)
}

pub fn handle_compressed_csi(message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let mut write_queries: Vec<WriteQuery> = Vec::new();

    // batch of readings
    let decompressed_data = inflate::inflate_bytes_zlib(&message.payload).unwrap();
    let frame_count = decompressed_data.len() / csi::COMPRESSED_CSI_FRAME_SIZE;

    if (decompressed_data.len() % csi::COMPRESSED_CSI_FRAME_SIZE) > 0 {
        println!("Could not determine the number of frames in compressed container from {:?} with size: {:?}.", message.addr, decompressed_data.len());
        return Err(RecvMessageError::MessageDecompressionError())
    }

    println!("Frames in container: {:?} from {}", frame_count, message.addr);

    for i in 0 .. frame_count {
        let protobuf_size = decompressed_data[csi::COMPRESSED_CSI_FRAME_SIZE * i] as usize;

        let protobuf_start = (csi::COMPRESSED_CSI_FRAME_SIZE * i) + 1;
        let protobuf_end = protobuf_start + protobuf_size;
        let protobuf_contents = &decompressed_data[ protobuf_start .. protobuf_end ];

        let Ok(msg) = parse_csi(protobuf_contents) else {
            println!("Invalid frame in decompressed array.");
            continue
        };

        // let reading = msg.reading.clone();
        let reading = msg;
        let sequence_identifier = reading.sequence_identifier;

        let key = format!("{}/{}", reading.mac.clone(), reading.antenna.clone());

        // match frame_map.get(&key) {
        //     Some(stored_frame) => {
        //         // Get interval
        //         let ret_sequence: i32 = i32::try_from(stored_frame.reading.sequence_identifier).ok().unwrap();
        //         // if ret_sequence > sequence_identifier {
        //         //     // Wraparound has occurred. Get diff minus u16 max.
        //         //     println!("wraparound check is fuck");
        //         //     let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
        //         //     reading.interval = sequence_identifier + ret_diff_from_max;
        //         // } else {
        //         //     reading.interval = sequence_identifier - ret_sequence;
        //         // }
        //
        //         // if ret_sequence > sequence_identifier {
        //         //     // Wraparound has occurred. Get diff minus u16 max.
        //         //     // println!("wraparound check is fuck");
        //         //     let ret_diff_from_max = u16::MAX as i32 - ret_sequence;
        //         //     reading.interval = sequence_identifier + ret_diff_from_max;
        //         //     println!("{:#?}", reading);
        //         // } else {
        //             reading.interval = sequence_identifier - ret_sequence;
        //         // }
        //
        //         // Get PCC
        //         let new_matrix = msg_reading.csi_matrix.clone();
        //         let corr = csi::get_correlation_coefficient(new_matrix, &stored_frame.csi_matrix).unwrap();
        //
        //         reading.correlation_coefficient = corr;
        //
        //         *frame_map.get_mut(&key).unwrap() = msg_reading;
        //     }
        //     None => {
        //         frame_map.insert(key, msg_reading);
        //         println!("Added new client with addr: {} src_mac: {} (time: {})", addr, reading.mac.clone(), reading.time.clone());
        //     }
        // }
        //
        write_queries.push(reading.into_query(csi::CSI_METRICS_MEASUREMENT));
    }

    Ok(write_queries)
}
