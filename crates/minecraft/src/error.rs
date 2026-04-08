use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("packet is too large: {size} bytes > {max_size} bytes")]
    PacketTooLarge { size: usize, max_size: usize },

    #[error("connection closed before a full packet arrived")]
    UnexpectedEof,

    #[error("invalid next state: {0}")]
    InvalidNextState(i32),

    #[error("failed to decode packet: {0}")]
    Decode(String),

    #[error("failed to encode packet: {0}")]
    Encode(String),

    #[error("invalid text json: {0}")]
    InvalidTextJson(String),
}

impl ProtocolError {
    pub fn decode(error: impl std::fmt::Display) -> Self {
        Self::Decode(error.to_string())
    }

    pub fn encode(error: impl std::fmt::Display) -> Self {
        Self::Encode(error.to_string())
    }
}
