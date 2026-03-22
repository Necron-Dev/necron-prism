use std::fmt;
use std::io;

#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    PacketTooLarge { size: usize, max_size: usize },
    UnexpectedEof,
    InvalidNextState(i32),
    Decode(String),
    Encode(String),
    InvalidTextJson(String),
}

impl ProtocolError {
    pub fn decode(error: impl fmt::Display) -> Self {
        Self::Decode(error.to_string())
    }

    pub fn encode(error: impl fmt::Display) -> Self {
        Self::Encode(error.to_string())
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::PacketTooLarge { size, max_size } => {
                write!(f, "packet is too large: {size} bytes > {max_size} bytes")
            }
            Self::UnexpectedEof => write!(f, "connection closed before a full packet arrived"),
            Self::InvalidNextState(state) => write!(f, "invalid next state: {state}"),
            Self::Decode(message) => write!(f, "failed to decode packet: {message}"),
            Self::Encode(message) => write!(f, "failed to encode packet: {message}"),
            Self::InvalidTextJson(message) => write!(f, "invalid text json: {message}"),
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ProtocolError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
