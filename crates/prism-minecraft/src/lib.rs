mod constants;
mod error;
mod packet_io;
mod protocol;
#[cfg(test)]
mod test;
mod types;

pub use constants::{
    INTENT_LOGIN, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE,
    MAX_STATUS_PACKET_SIZE,
};
pub use error::ProtocolError;
pub use packet_io::{FramedPacket, PacketIo};
pub use protocol::{
    PRISM_MAGIC_ID, decode_handshake, decode_login_hello, decode_ping_request,
    decode_pong_response, decode_status_request, decode_status_response, encode_handshake,
    encode_raw_frame, login_disconnect_packet, ping_request_packet, ping_response_packet,
    status_response_packet,
};
pub use types::{HandshakeInfo, RuntimeAddress};
pub use valence_protocol::uuid::Uuid;
