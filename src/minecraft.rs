use std::fmt;
use std::io::{self, Cursor, Read};

pub const MAX_HANDSHAKE_PACKET_SIZE: usize = 4096;
pub const MAX_STATUS_PACKET_SIZE: usize = 1024;
pub const MAX_LOGIN_PACKET_SIZE: usize = 1024;
const MAX_HOSTNAME_LEN: usize = 255;
const MAX_PLAYER_NAME_LEN: usize = 16;

pub const INTENT_STATUS: i32 = 1;
pub const INTENT_LOGIN: i32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramedPacket {
    pub payload: Vec<u8>,
    pub wire_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handshake {
    packet_id: i32,
    protocol_version: i32,
    server_address: String,
    server_port: u16,
    next_state: i32,
}

impl Handshake {
    pub fn decode(payload: &[u8]) -> Result<Self, ProtocolError> {
        let mut cursor = Cursor::new(payload);
        let packet_id = read_varint(&mut cursor)?;
        if packet_id != 0 {
            return Err(ProtocolError::InvalidPacketId(packet_id));
        }

        let protocol_version = read_varint(&mut cursor)?;
        if protocol_version <= 0 {
            return Err(ProtocolError::InvalidProtocolVersion(protocol_version));
        }

        let server_address = read_string(&mut cursor, MAX_HOSTNAME_LEN)?;
        let server_port = read_u16(&mut cursor)?;
        let next_state = read_varint(&mut cursor)?;
        if next_state <= 0 {
            return Err(ProtocolError::InvalidNextState(next_state));
        }

        if cursor.position() != payload.len() as u64 {
            return Err(ProtocolError::TrailingBytes);
        }

        Ok(Self {
            packet_id,
            protocol_version,
            server_address,
            server_port,
            next_state,
        })
    }

    pub fn rewrite(&mut self, host: &str, port: u16) {
        self.server_address = rewrite_host(&self.server_address, host);
        self.server_port = port;
    }

    pub fn protocol_version(&self) -> i32 {
        self.protocol_version
    }

    pub fn server_address(&self) -> &str {
        &self.server_address
    }

    pub fn server_port(&self) -> u16 {
        self.server_port
    }

    pub fn next_state(&self) -> i32 {
        self.next_state
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let mut payload = Vec::with_capacity(self.server_address.len() + 16);
        write_varint(&mut payload, self.packet_id)?;
        write_varint(&mut payload, self.protocol_version)?;
        write_string(&mut payload, &self.server_address)?;
        payload.extend_from_slice(&self.server_port.to_be_bytes());
        write_varint(&mut payload, self.next_state)?;

        let mut packet = Vec::with_capacity(payload.len() + 5);
        write_varint(&mut packet, payload.len() as i32)?;
        packet.extend_from_slice(&payload);
        Ok(packet)
    }
}

pub fn decode_status_request(payload: &[u8]) -> Result<(), ProtocolError> {
    let mut cursor = Cursor::new(payload);
    let packet_id = read_varint(&mut cursor)?;
    if packet_id != 0 {
        return Err(ProtocolError::InvalidPacketId(packet_id));
    }
    if cursor.position() != payload.len() as u64 {
        return Err(ProtocolError::TrailingBytes);
    }
    Ok(())
}

pub fn decode_ping_request(payload: &[u8]) -> Result<i64, ProtocolError> {
    let mut cursor = Cursor::new(payload);
    let packet_id = read_varint(&mut cursor)?;
    if packet_id != 1 {
        return Err(ProtocolError::InvalidPacketId(packet_id));
    }

    let mut bytes = [0_u8; 8];
    cursor.read_exact(&mut bytes)?;
    if cursor.position() != payload.len() as u64 {
        return Err(ProtocolError::TrailingBytes);
    }
    Ok(i64::from_be_bytes(bytes))
}

pub fn decode_login_start(payload: &[u8]) -> Result<String, ProtocolError> {
    let mut cursor = Cursor::new(payload);
    let packet_id = read_varint(&mut cursor)?;
    if packet_id != 0 {
        return Err(ProtocolError::InvalidPacketId(packet_id));
    }
    read_string(&mut cursor, MAX_PLAYER_NAME_LEN)
}

pub fn status_response_packet(json: &str) -> Result<Vec<u8>, ProtocolError> {
    encode_packet(|payload| {
        write_varint(payload, 0)?;
        write_string(payload, json)
    })
}

pub fn ping_response_packet(payload: i64) -> Result<Vec<u8>, ProtocolError> {
    encode_packet(|buffer| {
        write_varint(buffer, 1)?;
        buffer.extend_from_slice(&payload.to_be_bytes());
        Ok(())
    })
}

pub fn login_disconnect_packet(message_json: &str) -> Result<Vec<u8>, ProtocolError> {
    encode_packet(|payload| {
        write_varint(payload, 0)?;
        write_string(payload, message_json)
    })
}

pub fn read_framed_packet<R: Read>(
    reader: &mut R,
    max_size: usize,
) -> Result<Vec<u8>, ProtocolError> {
    Ok(read_framed_packet_with_len(reader, max_size)?.payload)
}

pub fn read_framed_packet_with_len<R: Read>(
    reader: &mut R,
    max_size: usize,
) -> Result<FramedPacket, ProtocolError> {
    let (packet_len, header_len) = read_varint_with_len(reader)?;
    if packet_len <= 0 {
        return Err(ProtocolError::InvalidPacketLength(packet_len));
    }

    let packet_len = packet_len as usize;
    if packet_len > max_size {
        return Err(ProtocolError::PacketTooLarge(packet_len));
    }

    let mut payload = vec![0; packet_len];
    reader.read_exact(&mut payload)?;
    Ok(FramedPacket {
        payload,
        wire_len: packet_len + header_len,
    })
}

fn encode_packet<F>(build_payload: F) -> Result<Vec<u8>, ProtocolError>
where
    F: FnOnce(&mut Vec<u8>) -> Result<(), ProtocolError>,
{
    let mut payload = Vec::new();
    build_payload(&mut payload)?;

    let mut packet = Vec::with_capacity(payload.len() + 5);
    write_varint(&mut packet, payload.len() as i32)?;
    packet.extend_from_slice(&payload);
    Ok(packet)
}

fn read_varint<R: Read>(reader: &mut R) -> Result<i32, ProtocolError> {
    Ok(read_varint_with_len(reader)?.0)
}

fn read_varint_with_len<R: Read>(reader: &mut R) -> Result<(i32, usize), ProtocolError> {
    let mut value = 0_i32;

    for shift in 0..5 {
        let mut byte = [0_u8; 1];
        reader.read_exact(&mut byte)?;
        value |= i32::from(byte[0] & 0x7f) << (shift * 7);

        if byte[0] & 0x80 == 0 {
            return Ok((value, shift + 1));
        }
    }

    Err(ProtocolError::VarIntTooLong)
}

fn write_varint(buffer: &mut Vec<u8>, value: i32) -> Result<(), ProtocolError> {
    if value < 0 {
        return Err(ProtocolError::NegativeVarInt(value));
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

fn read_string<R: Read>(reader: &mut R, max_len: usize) -> Result<String, ProtocolError> {
    let len = read_varint(reader)?;
    if len < 0 {
        return Err(ProtocolError::InvalidStringLength(len));
    }

    let len = len as usize;
    if len > max_len {
        return Err(ProtocolError::StringTooLong(len));
    }

    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(ProtocolError::InvalidUtf8)
}

fn write_string(buffer: &mut Vec<u8>, value: &str) -> Result<(), ProtocolError> {
    if value.len() > MAX_HOSTNAME_LEN {
        return Err(ProtocolError::StringTooLong(value.len()));
    }

    write_varint(buffer, value.len() as i32)?;
    buffer.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_u16<R: Read>(reader: &mut R) -> Result<u16, ProtocolError> {
    let mut bytes = [0_u8; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

fn rewrite_host(original: &str, new_host: &str) -> String {
    match original.split_once('\0') {
        Some((_, suffix)) => {
            let mut rewritten = String::with_capacity(new_host.len() + suffix.len() + 1);
            rewritten.push_str(new_host);
            rewritten.push('\0');
            rewritten.push_str(suffix);
            rewritten
        }
        None => new_host.to_owned(),
    }
}

#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    InvalidUtf8(std::string::FromUtf8Error),
    PacketTooLarge(usize),
    InvalidPacketLength(i32),
    InvalidPacketId(i32),
    InvalidProtocolVersion(i32),
    InvalidNextState(i32),
    InvalidStringLength(i32),
    StringTooLong(usize),
    NegativeVarInt(i32),
    VarIntTooLong,
    TrailingBytes,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidUtf8(error) => write!(f, "invalid UTF-8 in hostname: {error}"),
            Self::PacketTooLarge(size) => write!(f, "first packet is too large: {size} bytes"),
            Self::InvalidPacketLength(length) => write!(f, "invalid packet length: {length}"),
            Self::InvalidPacketId(id) => write!(f, "expected handshake packet id 0, got {id}"),
            Self::InvalidProtocolVersion(version) => {
                write!(f, "invalid protocol version: {version}")
            }
            Self::InvalidNextState(state) => write!(f, "invalid next state: {state}"),
            Self::InvalidStringLength(length) => write!(f, "invalid string length: {length}"),
            Self::StringTooLong(length) => write!(f, "string is too long: {length} bytes"),
            Self::NegativeVarInt(value) => write!(f, "cannot encode negative VarInt: {value}"),
            Self::VarIntTooLong => write!(f, "VarInt is too long"),
            Self::TrailingBytes => write!(f, "unexpected trailing bytes in handshake"),
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidUtf8(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ProtocolError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_handshake(server_address: &str, server_port: u16, next_state: i32) -> Handshake {
        Handshake {
            packet_id: 0,
            protocol_version: 760,
            server_address: server_address.to_owned(),
            server_port,
            next_state,
        }
    }

    #[test]
    fn handshake_round_trip() {
        let handshake = sample_handshake("example.com", 25565, 2);
        let packet = handshake.encode().unwrap();
        let payload =
            read_framed_packet(&mut Cursor::new(packet), MAX_HANDSHAKE_PACKET_SIZE).unwrap();
        let decoded = Handshake::decode(&payload).unwrap();

        assert_eq!(decoded, handshake);
    }

    #[test]
    fn rewrite_preserves_fml_suffix() {
        let mut handshake = sample_handshake("old.example\0FML\0", 25565, 2);
        handshake.rewrite("mc.hypixel.net", 25566);

        assert_eq!(handshake.server_address, "mc.hypixel.net\0FML\0");
        assert_eq!(handshake.server_port, 25566);
    }

    #[test]
    fn reject_non_handshake_packet() {
        let mut payload = Vec::new();
        write_varint(&mut payload, 1).unwrap();
        write_varint(&mut payload, 760).unwrap();
        write_string(&mut payload, "example.com").unwrap();
        payload.extend_from_slice(&25565_u16.to_be_bytes());
        write_varint(&mut payload, 2).unwrap();

        let error = Handshake::decode(&payload).unwrap_err();
        assert!(matches!(error, ProtocolError::InvalidPacketId(1)));
    }

    #[test]
    fn decode_status_request_packet() {
        let packet = encode_packet(|payload| write_varint(payload, 0)).unwrap();
        let payload = read_framed_packet(&mut Cursor::new(packet), MAX_STATUS_PACKET_SIZE).unwrap();

        decode_status_request(&payload).unwrap();
    }

    #[test]
    fn decode_login_start_keeps_modern_tail() {
        let packet = encode_packet(|payload| {
            write_varint(payload, 0)?;
            write_string(payload, "Steve")?;
            payload.extend_from_slice(&[1, 2, 3, 4]);
            Ok(())
        })
        .unwrap();
        let payload = read_framed_packet(&mut Cursor::new(packet), MAX_LOGIN_PACKET_SIZE).unwrap();

        let player_name = decode_login_start(&payload).unwrap();
        assert_eq!(player_name, "Steve");
    }

    #[test]
    fn encode_login_disconnect_packet() {
        let packet = login_disconnect_packet("{\"text\":\"blocked\"}").unwrap();
        let payload = read_framed_packet(&mut Cursor::new(packet), MAX_LOGIN_PACKET_SIZE).unwrap();
        let mut cursor = Cursor::new(payload);

        assert_eq!(read_varint(&mut cursor).unwrap(), 0);
        assert_eq!(
            read_string(&mut cursor, 128).unwrap(),
            "{\"text\":\"blocked\"}"
        );
    }
}
