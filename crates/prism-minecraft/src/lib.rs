mod constants;
mod error;
mod packet_io;
mod protocol;
#[cfg(test)]
mod test;
mod types;

pub use constants::{
    MAGIC, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE, MAX_STATUS_PACKET_SIZE,
};
pub use error::ProtocolError;
pub use packet_io::{FramedPacket, PacketIo};
pub use protocol::{
    decode_handshake, decode_login_hello, decode_request, encode_handshake, encode_raw_frame,
    login_disconnect_packet, ping_request_packet, ping_response_packet, status_response_packet,
};
pub use types::{HandshakeC2s, HandshakeNextState, LoginHelloInfo, RuntimeAddress};
pub use valence_protocol::VarInt;
pub use valence_protocol::packets::status::{
    QueryPingC2s, QueryPongS2c, QueryRequestC2s, QueryResponseS2c,
};
