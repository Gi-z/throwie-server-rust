use influxdb::{WriteQuery, InfluxDbWriteable};

use crate::{config, csi, telemetry};
use crate::error::RecvMessageError;
use crate::message::{MessageData, MessageType};

fn csi_metrics_measurement() -> String {
    config::get().lock().unwrap().influx.csi_metrics_measurement.clone()
}

pub fn handle_message(m: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    match m.format {
        MessageType::Telemetry => handle_telemetry(m),
        MessageType::CSI => handle_csi(m),
        MessageType::CSICompressed => handle_compressed_csi(m)
    }
}

fn handle_telemetry(message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let reading = parse_telemetry(&message.payload)?;
    Ok(vec![reading.into_query(&config::get().lock().unwrap().influx.sensor_telemetry_measurement)])
}

fn parse_telemetry(expected_payload: &[u8]) -> Result<telemetry::TelemetryReading, RecvMessageError> {
    let protobuf_parse_result = telemetry::parse_telemetry_protobuf(expected_payload)?;
    Ok(telemetry::get_reading(&protobuf_parse_result))
}

fn handle_csi(message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let reading = parse_csi(&message.payload)?;
    // let mapped_reading = map_frame(reading);

    // Ok(vec![mapped_reading.into_query(Self::csi_metrics_measurement())])
    Ok(vec![reading.into_query(csi_metrics_measurement())])
}

fn parse_csi(expected_payload: &[u8]) -> Result<csi::CSIReading, RecvMessageError>  {
    let protobuf_parse_result = csi::parse_csi_protobuf(expected_payload)?;
    csi::get_reading(&protobuf_parse_result)
}

fn handle_compressed_csi(message: MessageData) -> Result<Vec<WriteQuery>, RecvMessageError> {
    let mut write_queries: Vec<WriteQuery> = Vec::new();

    let compressed_frame_size = (config::get().lock().unwrap().message.csi_frame_size + 1) as usize;

    // batch of readings
    let decompressed_data = inflate::inflate_bytes_zlib(&message.payload).unwrap();
    let frame_count = decompressed_data.len() / compressed_frame_size;

    if (decompressed_data.len() % compressed_frame_size) > 0 {
        println!("Could not determine the number of frames in compressed container from {:?} with size: {:?}.", message.addr, decompressed_data.len());
        return Err(RecvMessageError::MessageDecompressionError())
    }

    // println!("Frames in container: {:?} from {}", frame_count, message.addr);

    for i in 0 .. frame_count {
        let protobuf_size = decompressed_data[compressed_frame_size * i] as usize;

        let protobuf_start = (compressed_frame_size * i) + 1;
        let protobuf_end = protobuf_start + protobuf_size;
        let protobuf_contents = &decompressed_data[ protobuf_start .. protobuf_end ];

        let Ok(msg) = csi::parse_csi_protobuf(protobuf_contents) else {
            println!("Invalid frame in decompressed array.");
            continue
        };

        let Ok(mut reading) = csi::get_reading(&msg) else {
            println!("Invalid frame in decompressed array.");
            continue
        };

        write_queries.push(reading.into_query(csi_metrics_measurement()));

        // let mapped_reading = self.map_frame(reading);

        // write_queries.push(mapped_reading.into_query(Self::csi_metrics_measurement()));
    }

    Ok(write_queries)
}