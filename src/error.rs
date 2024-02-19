use std::net::SocketAddr;
use prost::DecodeError;

use thiserror::Error;

use crate::throwie::CsiMessage;
use crate::throwie::TelemetryMessage;

#[derive(Error, Debug)]
pub enum RecvMessageError {
    #[error("socket.recv_from returned error.")]
    SocketRecvError(),

    #[error("Message decompression failed.")]
    MessageDecompressionError(),

    #[error("Could not determine format ({0}) for incoming message from {1} with size: {2}.")]
    MessageFormatDecodeError(u8, SocketAddr, usize),

    #[error("Could not determine type for incoming Message.")]
    MessageTypeDecodeError(),

    #[error("Failed to parse protobuf from buffer contents.")]
    ProtobufParseError(#[from] DecodeError),

    #[error("Failed to build CSIReading from protobuf:")]
    CSIReadingGenerateError(),

    #[error("Failed to build CSIReading from protobuf:")]
    TelemetryReadingGenerateError(),

    #[error("Failed to generate a valid CSI matrix from protobuf:")]
    CSIMatrixParseError(),

    #[error("Failed to calculate PCC for given frames.")]
    PCCCalcError(),
}