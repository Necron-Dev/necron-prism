use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{Cursor, Read};
use std::str::FromStr;

use smallvec::SmallVec;
use valence_protocol::encode::PacketEncoder;
use valence_protocol::packets::login::{LoginDisconnectS2c, LoginHelloC2s};
use valence_protocol::packets::status::{QueryPingC2s, QueryPongS2c, QueryResponseS2c};
use valence_protocol::uuid::Uuid;
use valence_protocol::{Decode, Encode, Packet, Text, VarInt};

use crate::RuntimeAddress;
pub use crate::types::{HandshakeC2s, LoginHelloInfo};

use super::error::ProtocolError;
use super::packet_io::FramedPacket;
thread_local! {
    static ENCODER: RefCell<PacketEncoder> = RefCell::new(PacketEncoder::new());
}

impl HandshakeC2s {
    pub fn rewrite_addr(&mut self, addr: &RuntimeAddress) -> Result<(), String> {
        let host = addr.host();
        let port = addr.port();

        if let Some(pos) = self.server_address.find('\0') {
            let suffix = &self.server_address[pos..];
            let mut rewritten = String::with_capacity(host.len() + suffix.len());
            rewritten.push_str(host);
            rewritten.push_str(suffix);
            self.server_address = rewritten;
        } else {
            self.server_address = host.to_owned();
        }
        self.server_port = port;
        Ok(())
    }
}

pub fn decode_handshake(frame: &FramedPacket) -> Result<HandshakeC2s, ProtocolError> {
    decode_request::<HandshakeC2s>(frame)
}

pub fn decode_request<'a, T: Packet + Decode<'a>>(
    frame: &'a FramedPacket,
) -> Result<T, ProtocolError> {
    frame.frame.decode::<T>().map_err(ProtocolError::decode)
}

pub fn decode_login_hello(frame: &FramedPacket) -> Result<LoginHelloInfo, ProtocolError> {
    if let Ok(packet) = decode_request::<LoginHelloC2s>(frame) {
        return Ok(LoginHelloInfo {
            username: packet.username.to_owned(),
            profile_id: packet.profile_id,
        });
    }

    if frame.frame.id != 0 {
        return Err(ProtocolError::decode("invalid login packet id"));
    }

    let mut cursor = Cursor::new(frame.frame.body.as_ref());
    let username = decode_mc_string(&mut cursor, 16)?;

    let remaining = frame.frame.body.len() - cursor.position() as usize;
    let profile_id = if remaining >= 16 {
        let first_byte = frame.frame.body[cursor.position() as usize];
        if remaining == 17 && (first_byte == 0 || first_byte == 1) {
            decode_uuid_with_flag(&mut cursor).ok()
        } else {
            let mut uuid = [0u8; 16];
            cursor
                .read_exact(&mut uuid)
                .map_err(ProtocolError::decode)?;
            Some(Uuid::from_bytes(uuid))
        }
    } else if remaining > 0 {
        decode_uuid_with_flag(&mut cursor).ok()
    } else {
        None
    };

    Ok(LoginHelloInfo {
        username,
        profile_id,
    })
}

fn decode_mc_string(cursor: &mut Cursor<&[u8]>, max_len: usize) -> Result<String, ProtocolError> {
    let len = VarInt::decode_partial(&mut *cursor).map_err(ProtocolError::decode)? as usize;
    if len > max_len * 4 {
        return Err(ProtocolError::decode("string too long"));
    }
    let mut buf = vec![0; len];
    cursor.read_exact(&mut buf).map_err(ProtocolError::decode)?;
    String::from_utf8(buf).map_err(|_| ProtocolError::decode("invalid utf8"))
}

fn decode_uuid_with_flag(cursor: &mut Cursor<&[u8]>) -> Result<Uuid, ProtocolError> {
    let mut flag = [0u8; 1];
    cursor
        .read_exact(&mut flag)
        .map_err(ProtocolError::decode)?;
    if flag[0] == 0 {
        return Err(ProtocolError::decode("no uuid"));
    }
    let mut uuid = [0u8; 16];
    cursor
        .read_exact(&mut uuid)
        .map_err(ProtocolError::decode)?;
    Ok(Uuid::from_bytes(uuid))
}

pub fn encode_raw_frame(frame: &FramedPacket) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    let packet_len = VarInt(frame.frame.id).written_size() + frame.frame.body.len();
    let wire_len = VarInt(packet_len as i32).written_size() + packet_len;
    let mut output = SmallVec::with_capacity(wire_len);
    VarInt(packet_len as i32)
        .encode(&mut output)
        .map_err(ProtocolError::encode)?;
    VarInt(frame.frame.id)
        .encode(&mut output)
        .map_err(ProtocolError::encode)?;
    output.extend_from_slice(frame.frame.body.as_ref());
    Ok(output)
}

pub fn ping_request_packet(payload: u64) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    encode_packet(&QueryPingC2s { payload })
}

pub fn encode_handshake(handshake: &HandshakeC2s) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    encode_packet(handshake)
}

pub fn status_response_packet(json: &str) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    encode_packet(&QueryResponseS2c { json })
}

pub fn ping_response_packet(payload: u64) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    encode_packet(&QueryPongS2c { payload })
}

pub fn login_disconnect_packet(message_json: &str) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    let reason =
        Text::from_str(message_json).map_err(|e| ProtocolError::InvalidTextJson(e.to_string()))?;
    encode_packet(&LoginDisconnectS2c {
        reason: Cow::Owned(reason),
    })
}

fn encode_packet<P>(packet: &P) -> Result<SmallVec<[u8; 256]>, ProtocolError>
where
    P: Packet + valence_protocol::Encode,
{
    ENCODER.with(|cell| {
        let mut encoder = cell.borrow_mut();
        encoder.clear();
        encoder
            .append_packet(packet)
            .map_err(ProtocolError::encode)?;
        let bytes = encoder.take();
        let mut output = SmallVec::with_capacity(bytes.len());
        output.extend_from_slice(&bytes);
        Ok(output)
    })
}
