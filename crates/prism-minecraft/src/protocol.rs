use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{Cursor, Read};
use std::str::FromStr;

use smallvec::SmallVec;
use valence_protocol::encode::PacketEncoder;
use valence_protocol::packets::login::{LoginDisconnectS2c, LoginHelloC2s};
use valence_protocol::packets::status::{
    QueryPingC2s, QueryPongS2c, QueryRequestC2s, QueryResponseS2c,
};
use valence_protocol::uuid::Uuid;
use valence_protocol::{Decode, Encode, Packet, PacketState, Text, VarInt, packet_id};

use super::constants::{INTENT_LOGIN, INTENT_STATUS, INTENT_TRANSFER};
use super::error::ProtocolError;
use super::packet_io::FramedPacket;
use super::types::HandshakeInfo;

thread_local! {
    static ENCODER: RefCell<PacketEncoder> = RefCell::new(PacketEncoder::new());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginHelloInfo {
    pub username: String,
    pub profile_id: Option<Uuid>,
}

#[derive(Clone, Debug, Encode, Decode, Packet)]
#[packet(id = packet_id::HANDSHAKE_C2S, state = PacketState::Handshaking)]
pub struct HandshakeC2s<'a> {
    pub protocol_version: VarInt,
    pub server_address: &'a str,
    pub server_port: u16,
    pub next_state: HandshakeNextState,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum HandshakeNextState {
    #[packet(tag = 1)]
    Status,
    #[packet(tag = 2)]
    Login,
    #[packet(tag = 3)]
    Transfer,
}

pub fn decode_handshake(frame: &FramedPacket) -> Result<HandshakeInfo, ProtocolError> {
    let packet = frame
        .frame
        .decode::<HandshakeC2s<'_>>()
        .map_err(ProtocolError::decode)?;
    Ok(HandshakeInfo {
        protocol_version: packet.protocol_version.0,
        server_address: packet.server_address.to_owned(),
        server_port: packet.server_port,
        next_state: match packet.next_state {
            HandshakeNextState::Status => INTENT_STATUS,
            HandshakeNextState::Login => INTENT_LOGIN,
            HandshakeNextState::Transfer => INTENT_TRANSFER,
        },
    })
}

pub fn decode_status_request(frame: &FramedPacket) -> Result<(), ProtocolError> {
    frame
        .frame
        .decode::<QueryRequestC2s>()
        .map(|_| ())
        .map_err(ProtocolError::decode)
}

pub fn decode_ping_request(frame: &FramedPacket) -> Result<u64, ProtocolError> {
    frame
        .frame
        .decode::<QueryPingC2s>()
        .map(|p| p.payload)
        .map_err(ProtocolError::decode)
}

pub fn decode_login_hello(frame: &FramedPacket) -> Result<LoginHelloInfo, ProtocolError> {
    if let Ok(packet) = frame.frame.decode::<LoginHelloC2s<'_>>() {
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

pub fn decode_status_response(frame: &FramedPacket) -> Result<String, ProtocolError> {
    frame
        .frame
        .decode::<QueryResponseS2c<'_>>()
        .map(|p| p.json.to_owned())
        .map_err(ProtocolError::decode)
}

pub fn decode_pong_response(frame: &FramedPacket) -> Result<u64, ProtocolError> {
    frame
        .frame
        .decode::<QueryPongS2c>()
        .map(|p| p.payload)
        .map_err(ProtocolError::decode)
}

pub fn ping_request_packet(payload: u64) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    encode_packet(&QueryPingC2s { payload })
}

pub fn encode_handshake(handshake: &HandshakeInfo) -> Result<SmallVec<[u8; 256]>, ProtocolError> {
    let packet = HandshakeC2s {
        protocol_version: VarInt(handshake.protocol_version),
        server_address: &handshake.server_address,
        server_port: handshake.server_port,
        next_state: match handshake.next_state {
            INTENT_STATUS => HandshakeNextState::Status,
            INTENT_LOGIN => HandshakeNextState::Login,
            INTENT_TRANSFER => HandshakeNextState::Transfer,
            _ => return Err(ProtocolError::InvalidNextState(handshake.next_state)),
        },
    };
    encode_packet(&packet)
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

pub const PRISM_MAGIC_ID: i32 = 0xDEAD0721u32 as i32;

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
