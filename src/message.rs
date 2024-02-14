

pub fn parse_telemetry_message(expected_payload: &[u8]) -> Result<CSIMessage, protobuf::Error>  {
    CSIMessage::parse_from_bytes(expected_protobuf)
}

pub fn parse_csi_message(expected_payload: &[u8]) -> Result<CSIMessage, protobuf::Error>  {
    CSIMessage::parse_from_bytes(expected_protobuf)
}

pub fn parse_compressed_csi_message(expected_payload: &[u8]) -> Result<CSIMessage, protobuf::Error>  {
    CSIMessage::parse_from_bytes(expected_protobuf)
}