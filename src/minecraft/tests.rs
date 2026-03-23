use std::io::Cursor;
use std::str::FromStr;

use valence_protocol::packets::login::LoginDisconnectS2c;
use valence_protocol::packets::login::LoginHelloC2s;
use valence_protocol::packets::status::{QueryPongS2c, QueryRequestC2s, QueryResponseS2c};
use valence_protocol::uuid::Uuid;
use valence_protocol::Text;
use valence_protocol::WritePacket;

use super::protocol::LoginHelloInfo;
use super::{
    decode_handshake, decode_login_hello, decode_ping_request, decode_status_request,
    encode_handshake, encode_raw_frame, login_disconnect_packet, ping_response_packet, status_response_packet,
    HandshakeInfo, PacketIo, ProtocolError, MAX_HANDSHAKE_PACKET_SIZE,
    MAX_LOGIN_PACKET_SIZE, MAX_STATUS_PACKET_SIZE,
};

fn sample_handshake(server_address: &str, server_port: u16, next_state: i32) -> HandshakeInfo {
    HandshakeInfo {
        protocol_version: 760,
        server_address: server_address.to_owned(),
        server_port,
        next_state,
    }
}

#[test]
fn handshake_round_trip() {
    let handshake = sample_handshake("example.com", 25565, 2);
    let packet = encode_handshake(&handshake).unwrap();
    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_HANDSHAKE_PACKET_SIZE)
        .unwrap();
    let decoded = decode_handshake(&frame).unwrap();

    assert_eq!(decoded, handshake);
}

#[test]
fn handshake_transfer_round_trip() {
    let handshake = sample_handshake("example.com", 25565, 3);
    let packet = encode_handshake(&handshake).unwrap();
    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_HANDSHAKE_PACKET_SIZE)
        .unwrap();
    let decoded = decode_handshake(&frame).unwrap();

    assert_eq!(decoded, handshake);
}

#[test]
fn rewrite_preserves_fml_suffix() {
    let mut handshake = sample_handshake("old.example\0FML\0", 25565, 2);
    handshake.rewrite_addr("mc.hypixel.net:25566").unwrap();

    assert_eq!(handshake.server_address, "mc.hypixel.net\0FML\0");
    assert_eq!(handshake.server_port, 25566);
}

#[test]
fn reject_invalid_next_state() {
    let handshake = sample_handshake("example.com", 25565, 42);
    let error = encode_handshake(&handshake).unwrap_err();

    assert!(matches!(error, ProtocolError::InvalidNextState(42)));
}

#[test]
fn decode_status_request_packet() {
    let mut packet = Vec::new();
    valence_protocol::encode::PacketWriter::new(&mut packet, None)
        .write_packet_fallible(&QueryRequestC2s)
        .unwrap();
    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_STATUS_PACKET_SIZE)
        .unwrap();

    decode_status_request(&frame).unwrap();
}

#[test]
fn encode_status_response_packet() {
    let packet = status_response_packet("{\"text\":\"hello\"}").unwrap();
    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_STATUS_PACKET_SIZE)
        .unwrap();
    let decoded = frame.frame.decode::<QueryResponseS2c<'_>>().unwrap();

    assert_eq!(decoded.json, "{\"text\":\"hello\"}");
}

#[test]
fn ping_packet_round_trip() {
    let packet = ping_response_packet(42).unwrap();
    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_STATUS_PACKET_SIZE)
        .unwrap();

    assert_eq!(decode_ping_request(&frame).unwrap(), 42);

    let decoded = frame.frame.decode::<QueryPongS2c>().unwrap();
    assert_eq!(decoded.payload, 42);
}

#[test]
fn encode_login_disconnect_packet() {
    let kick_json = "{\"text\":\"blocked\"}";
    let packet = login_disconnect_packet(kick_json).unwrap();
    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_LOGIN_PACKET_SIZE)
        .unwrap();
    let decoded = frame.frame.decode::<LoginDisconnectS2c<'_>>().unwrap();
    let expected = Text::from_str(kick_json).unwrap();

    assert_eq!(&*decoded.reason, &expected);
}

#[test]
fn decode_login_hello_modern_packet() {
    let profile_id = Uuid::from_bytes([1; 16]);
    let mut packet = Vec::new();

    valence_protocol::encode::PacketWriter::new(&mut packet, None)
        .write_packet_fallible(&LoginHelloC2s {
            username: "Me0wo",
            profile_id: Some(profile_id),
        })
        .unwrap();

    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_LOGIN_PACKET_SIZE)
        .unwrap();

    assert_eq!(
        decode_login_hello(&frame).unwrap(),
        LoginHelloInfo {
            username: "Me0wo".to_owned(),
            profile_id: Some(profile_id),
        }
    );
}

#[test]
fn decode_login_hello_legacy_packet_without_profile_id() {
    let mut packet = vec![0, 5];
    packet.extend_from_slice(b"Me0wo");
    packet.insert(0, packet.len() as u8);

    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet), MAX_LOGIN_PACKET_SIZE)
        .unwrap();

    assert_eq!(
        decode_login_hello(&frame).unwrap(),
        LoginHelloInfo {
            username: "Me0wo".to_owned(),
            profile_id: None,
        }
    );
}

#[test]
fn encode_raw_frame_round_trip() {
    let profile_id = Uuid::from_bytes([2; 16]);
    let mut packet = Vec::new();

    valence_protocol::encode::PacketWriter::new(&mut packet, None)
        .write_packet_fallible(&LoginHelloC2s {
            username: "Me0wo",
            profile_id: Some(profile_id),
        })
        .unwrap();

    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet.clone()), MAX_LOGIN_PACKET_SIZE)
        .unwrap();
    let encoded = encode_raw_frame(&frame).unwrap();

    assert_eq!(encoded, packet);
}

#[test]
fn framed_packet_reports_wire_len() {
    let handshake = sample_handshake("example.com", 25565, 2);
    let packet = encode_handshake(&handshake).unwrap();
    let frame = PacketIo::new()
        .read_frame(&mut Cursor::new(packet.clone()), MAX_HANDSHAKE_PACKET_SIZE)
        .unwrap();

    assert_eq!(frame.wire_len, packet.len());
}
