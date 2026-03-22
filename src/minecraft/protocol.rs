use std::borrow::Cow;
use std::io::{Cursor, Read};
use std::str::FromStr;

use valence_protocol::encode::PacketEncoder;
use valence_protocol::packets::handshaking::handshake_c2s::HandshakeNextState;
use valence_protocol::packets::handshaking::HandshakeC2s;
use valence_protocol::packets::login::{LoginDisconnectS2c, LoginHelloC2s};
use valence_protocol::packets::status::{
    QueryPingC2s, QueryPongS2c, QueryRequestC2s, QueryResponseS2c,
};
use valence_protocol::uuid::Uuid;
use valence_protocol::{Packet, Text, VarInt};

use super::constants::{INTENT_LOGIN, INTENT_STATUS};
use super::error::ProtocolError;
use super::packet_io::FramedPacket;
use super::types::HandshakeInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginHelloInfo {
    pub username: String,
    pub profile_id: Option<Uuid>,
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
        next_state: next_state_to_int(packet.next_state),
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
        Err(modern_error) => decode_legacy_login_hello(frame).map_err(|legacy_error| {
            ProtocolError::decode(format!(
                "{modern_error}; legacy login fallback failed: {legacy_error}"
            ))
        }),
    }
}

pub fn encode_raw_frame(frame: &FramedPacket) -> Result<Vec<u8>, ProtocolError> {
    let id_len = varint_len(frame.frame.id);
    let packet_len = id_len + frame.frame.body.len();

    let mut output = Vec::with_capacity(varint_len(packet_len as i32) + packet_len);
    write_varint(&mut output, packet_len as i32)?;
    write_varint(&mut output, frame.frame.id)?;
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
    let packet = HandshakeC2s {
        protocol_version: VarInt(handshake.protocol_version),
        server_address: &handshake.server_address,
        server_port: handshake.server_port,
        next_state: int_to_next_state(handshake.next_state)?,
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

fn encode_packet<P>(packet: &P) -> Result<Vec<u8>, ProtocolError>
where
    P: Packet + valence_protocol::Encode,
{
    let mut encoder = PacketEncoder::new();
    encoder
        .append_packet(packet)
        .map_err(ProtocolError::encode)?;
    Ok(encoder.take().to_vec())
}

fn decode_legacy_login_hello(frame: &FramedPacket) -> Result<LoginHelloInfo, ProtocolError> {
    if frame.frame.id != 0 {
        return Err(ProtocolError::decode(format!(
            "unexpected login packet id {}",
            frame.frame.id
        )));
    }

    let mut cursor = Cursor::new(frame.frame.body.as_ref());
    let username = read_mc_string(&mut cursor, 64)?;
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
                    "legacy login packet ended before profile id bytes",
                ));
            }
        }
        16 => Some(read_uuid(&mut cursor)?),
        17 => {
            let mut flag = [0_u8; 1];
            cursor.read_exact(&mut flag)?;
            match flag[0] {
                0 => None,
                1 => Some(read_uuid(&mut cursor)?),
                value => {
                    return Err(ProtocolError::decode(format!(
                        "invalid profile id presence flag {value}"
                    )));
                }
            }
        }
        value => {
            return Err(ProtocolError::decode(format!(
                "unexpected legacy login payload of {value} trailing bytes"
            )));
        }
    };

    Ok(LoginHelloInfo {
        username,
        profile_id,
    })
}

fn read_mc_string<R: Read>(reader: &mut R, max_len: usize) -> Result<String, ProtocolError> {
    let len = read_varint(reader)?;
    if len < 0 {
        return Err(ProtocolError::decode(format!(
            "negative string length {len}"
        )));
    }

    let len = len as usize;
    if len > max_len {
        return Err(ProtocolError::decode(format!(
            "string length {len} exceeds {max_len}"
        )));
    }

    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(ProtocolError::decode)
}

fn read_uuid<R: Read>(reader: &mut R) -> Result<Uuid, ProtocolError> {
    let mut bytes = [0_u8; 16];
    reader.read_exact(&mut bytes)?;
    Ok(Uuid::from_bytes(bytes))
}

fn read_varint<R: Read>(reader: &mut R) -> Result<i32, ProtocolError> {
    let mut value = 0_i32;

    for shift in 0..5 {
        let mut byte = [0_u8; 1];
        reader.read_exact(&mut byte)?;
        value |= i32::from(byte[0] & 0x7f) << (shift * 7);

        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }

    Err(ProtocolError::decode("VarInt is too long"))
}

fn write_varint(buffer: &mut Vec<u8>, value: i32) -> Result<(), ProtocolError> {
    if value < 0 {
        return Err(ProtocolError::encode(format!(
            "negative VarInt is unsupported: {value}"
        )));
    }

    let mut value = value as u32;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buffer.push(byte);
        if value == 0 {
            return Ok(());
        }
    }
}

fn varint_len(value: i32) -> usize {
    let mut value = value as u32;
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

fn next_state_to_int(state: HandshakeNextState) -> i32 {
    match state {
        HandshakeNextState::Status => INTENT_STATUS,
        HandshakeNextState::Login => INTENT_LOGIN,
    }
}

fn int_to_next_state(state: i32) -> Result<HandshakeNextState, ProtocolError> {
    match state {
        INTENT_STATUS => Ok(HandshakeNextState::Status),
        INTENT_LOGIN => Ok(HandshakeNextState::Login),
        _ => Err(ProtocolError::InvalidNextState(state)),
    }
}
