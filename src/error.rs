use std::net::SocketAddr;

use thiserror::Error;

include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));
use csimsg::CSIMessage;
use telemetrymsg::TelemetryMessage;

#[derive(Error, Debug)]
pub enum MessageDecodeError {
    #[error("Could not determine type for incoming Message.")]
    MessageTypeDecodeError(),
}

#[derive(Error, Debug)]
pub enum RecvMessageError {
    #[error("socket.recv_from returned error.")]
    SocketRecvError(),

    #[error("Message decompression failed.")]
    MessageDecompressionError(),

    #[error("Could not determine format ({0}) for incoming message from {1} with size: {2}.")]
    MessageFormatDecodeError(u8, SocketAddr, usize),

    #[error("Failed to parse protobuf from buffer contents.")]
    ProtobufParseError(#[from] protobuf::Error),

    #[error("Failed to build CSIReading from protobuf: {0}")]
    TelemetryReadingGenerateError(TelemetryMessage),

    #[error("Failed to build CSIReading from protobuf: {0}")]
    CSIReadingGenerateError(CSIMessage),

    #[error("Failed to generate a valid CSI matrix from protobuf: {0}.")]
    CSIMatrixParseError(CSIMessage),

    #[error("Failed to calculate PCC for given frames.")]
    PCCCalcError(),
}

// #[derive(Error, Debug)]
// pub enum RetrieveMsgError {
//     #[error("Could not receive data from UDP socket.")]
//     SocketRecvError(),
//
//     #[error("CSI expected_size: {0} is larger than allocated buffer: {1}.")]
//     MsgTooBigError(usize, usize),
//
//     #[error("Failed to parse protobuf from buffer contents.")]
//     ProtobufParseError(#[from] protobuf::Error),
//
//
//
//
// }