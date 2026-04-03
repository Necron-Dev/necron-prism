use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{Cursor, Read};
use std::str::FromStr;

use valence_protocol::encode::PacketEncoder;
use valence_protocol::packets::login::{LoginDisconnectS2c, LoginHelloC2s};
use valence_protocol::packets::status::{
    QueryPingC2s, QueryPongS2c, QueryRequestC2s, QueryResponseS2c,
};
use valence_protocol::uuid::Uuid;
use valence_protocol::{packet_id, Decode, Encode, Packet, PacketState, Text, VarInt};

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
pub struct HandshakeC2sNew<'a> {
    pub protocol_version: VarInt,
    pub server_address: &'a str,
    pub server_port: u16,
    pub next_state: HandshakeNextStateNew,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum HandshakeNextStateNew {
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
        .decode::<HandshakeC2sNew<'_>>()
        .map_err(ProtocolError::decode)?;

    Ok(HandshakeInfo {
        protocol_version: packet.protocol_version.0,
        server_address: packet.server_address.to_owned(),
        server_port: packet.server_port,
        next_state: match packet.next_state {
            HandshakeNextStateNew::Status => INTENT_STATUS,
            HandshakeNextStateNew::Login => INTENT_LOGIN,
            HandshakeNextStateNew::Transfer => INTENT_TRANSFER,
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
        .map(|packet| packet.payload)
        .map_err(ProtocolError::decode)
}

pub fn decode_login_hello(frame: &FramedPacket) -> Result<LoginHelloInfo, ProtocolError> {
    match frame.frame.decode::<LoginHelloC2s<'_>>() {
        Ok(packet) => Ok(LoginHelloInfo {
            username: packet.username.to_owned(),
            profile_id: packet.profile_id,
        }),
        Err(modern_error) => {
            // Legacy login fallback (inlined from decode_legacy_login_hello)
            if frame.frame.id != 0 {
                return Err(ProtocolError::decode(format!(
                    "{modern_error}; unexpected login packet id {}",
                    frame.frame.id
                )));
            }

            let mut cursor = Cursor::new(frame.frame.body.as_ref());

            // Inline read_mc_string(&mut cursor, 64)
            let len = VarInt::decode_partial(&mut cursor).map_err(ProtocolError::decode)?;
            if len < 0 {
                return Err(ProtocolError::decode(format!(
                    "{modern_error}; legacy login fallback failed: negative string length {len}"
                )));
            }
            let len = len as usize;
            if len > 64 {
                return Err(ProtocolError::decode(format!(
                    "{modern_error}; legacy login fallback failed: string length {len} exceeds 64"
                )));
            }
            let mut username_bytes = vec![0; len];
            cursor
                .read_exact(&mut username_bytes)
                .map_err(ProtocolError::decode)?;
            let username = String::from_utf8(username_bytes).map_err(|_| {
                ProtocolError::decode(format!(
                    "{modern_error}; legacy login fallback failed: invalid utf8"
                ))
            })?;

            let remaining = frame.frame.body.len() - cursor.position() as usize;

            let profile_id = match remaining {
                0 => None,
                1 => {
                    let mut flag = [0_u8; 1];
                    cursor.read_exact(&mut flag)?;
                    if flag[0] == 0 {
                        None
                    } else {
                        return Err(ProtocolError::decode(
                            format!("{modern_error}; legacy login fallback failed: legacy login packet ended before profile id bytes")
                        ));
                    }
                }
                16 => {
                    // Inline read_uuid(&mut cursor)
                    let mut bytes = [0_u8; 16];
                    cursor
                        .read_exact(&mut bytes)
                        .map_err(ProtocolError::decode)?;
                    Some(Uuid::from_bytes(bytes))
                }
                17 => {
                    let mut flag = [0_u8; 1];
                    cursor.read_exact(&mut flag)?;
                    match flag[0] {
                        0 => None,
                        1 => {
                            // Inline read_uuid(&mut cursor)
                            let mut bytes = [0_u8; 16];
                            cursor
                                .read_exact(&mut bytes)
                                .map_err(ProtocolError::decode)?;
                            Some(Uuid::from_bytes(bytes))
                        }
                        value => {
                            return Err(ProtocolError::decode(format!(
                                "{modern_error}; legacy login fallback failed: invalid profile id presence flag {value}"
                            )));
                        }
                    }
                }
                value => {
                    return Err(ProtocolError::decode(format!(
                        "{modern_error}; legacy login fallback failed: unexpected legacy login payload of {value} trailing bytes"
                    )));
                }
            };

            Ok(LoginHelloInfo {
                username,
                profile_id,
            })
        }
    }
}

pub fn encode_raw_frame(frame: &FramedPacket) -> Result<Vec<u8>, ProtocolError> {
    let id_len = VarInt(frame.frame.id).written_size();
    let packet_len = id_len + frame.frame.body.len();

    let mut output = Vec::with_capacity(VarInt(packet_len as i32).written_size() + packet_len);
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
        .map(|packet| packet.json.to_owned())
        .map_err(ProtocolError::decode)
}

pub fn decode_pong_response(frame: &FramedPacket) -> Result<u64, ProtocolError> {
    frame
        .frame
        .decode::<QueryPongS2c>()
        .map(|packet| packet.payload)
        .map_err(ProtocolError::decode)
}

pub fn ping_request_packet(payload: u64) -> Result<Vec<u8>, ProtocolError> {
    encode_packet(&QueryPingC2s { payload })
}

pub fn encode_handshake(handshake: &HandshakeInfo) -> Result<Vec<u8>, ProtocolError> {
    let packet = HandshakeC2sNew {
        protocol_version: VarInt(handshake.protocol_version),
        server_address: &handshake.server_address,
        server_port: handshake.server_port,
        next_state: match handshake.next_state {
            INTENT_STATUS => HandshakeNextStateNew::Status,
            INTENT_LOGIN => HandshakeNextStateNew::Login,
            INTENT_TRANSFER => HandshakeNextStateNew::Transfer,
            _ => return Err(ProtocolError::InvalidNextState(handshake.next_state)),
        },
    };
    encode_packet(&packet)
}

pub fn status_response_packet(json: &str) -> Result<Vec<u8>, ProtocolError> {
    encode_packet(&QueryResponseS2c { json })
}

pub fn ping_response_packet(payload: u64) -> Result<Vec<u8>, ProtocolError> {
    encode_packet(&QueryPongS2c { payload })
}

pub fn login_disconnect_packet(message_json: &str) -> Result<Vec<u8>, ProtocolError> {
    let reason = Text::from_str(message_json)
        .map_err(|error| ProtocolError::InvalidTextJson(error.to_string()))?;

    encode_packet(&LoginDisconnectS2c {
        reason: Cow::Owned(reason),
    })
}

pub const PRISM_MAGIC_ID: i32 = 0xDEAD0721u32 as i32;

fn encode_packet<P>(packet: &P) -> Result<Vec<u8>, ProtocolError>
where
    P: Packet + valence_protocol::Encode,
{
    ENCODER.with(|cell| {
        let mut encoder = cell.borrow_mut();
        encoder.clear();
        encoder
            .append_packet(packet)
            .map_err(ProtocolError::encode)?;
        Ok(encoder.take().to_vec())
    })
}
