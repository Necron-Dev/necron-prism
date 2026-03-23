use std::io::{self, Write};
use std::net::TcpStream;

use crate::minecraft::HandshakeInfo;
use crate::proxy::config::TransportConfig;
use crate::proxy::players::{PlayerRegistry, PlayerState};
use crate::proxy::stats::ConnectionTraffic;
use crate::proxy::template;

use super::rewrite::rewrite_json;

pub fn serve_legacy_ping(
    client: &mut TcpStream,
    transport: &TransportConfig,
    players: &PlayerRegistry,
    connection_id: u64,
) -> io::Result<ConnectionTraffic> {
    let upstream_json = if matches!(
        transport.motd.mode,
        crate::proxy::config::MotdMode::Upstream
    ) {
        fetch_upstream_status_json(transport).unwrap_or_else(|_| {
            transport
                .motd
                .local_json
                .clone()
                .unwrap_or_else(|| "{}".to_string())
        })
    } else {
        template::render(
            transport.motd.local_json.as_deref().unwrap_or("{}"),
            players,
        )
    };

    let motd_json = rewrite_json(
        &upstream_json,
        transport.motd.protocol_mode,
        763,
        &transport.motd.favicon,
        None,
    );
    let legacy_raw = extract_legacy_text(&motd_json);
    let response = encode_legacy_response(&legacy_raw);
    client.write_all(&response)?;

    players.set_state(connection_id, PlayerState::StatusServedLocally);

    Ok(ConnectionTraffic {
        upload_bytes: 1,
        download_bytes: response.len() as u64,
    })
}

fn fetch_upstream_status_json(transport: &TransportConfig) -> io::Result<String> {
    let address = transport.motd.upstream_addr.as_deref().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "missing MOTD upstream address")
    })?;
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(transport.motd.upstream_ping_timeout))?;

    let handshake = HandshakeInfo {
        protocol_version: 763,
        server_address: address.to_string(),
        server_port: extract_port(address).unwrap_or(25565),
        next_state: 1,
    };
    let mut request = crate::minecraft::encode_handshake(&handshake).map_err(io::Error::other)?;
    request.extend_from_slice(&[1, 0]);
    stream.write_all(&request)?;

    let mut packet_io = crate::minecraft::PacketIo::new();
    let frame = packet_io
        .read_frame(&mut stream, 64 * 1024)
        .map_err(io::Error::other)?;
    crate::minecraft::decode_status_response(&frame).map_err(io::Error::other)
}

fn encode_legacy_response(value: &str) -> Vec<u8> {
    let utf16: Vec<u16> = value.encode_utf16().collect();
    let mut response = Vec::with_capacity(3 + utf16.len() * 2);
    response.push(0xFF);
    response.extend_from_slice(&(utf16.len() as u16).to_be_bytes());
    for word in utf16 {
        response.extend_from_slice(&word.to_be_bytes());
    }
    response
}

fn extract_legacy_text(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|value| {
            value
                .pointer("/description/text")
                .and_then(|value| value.as_str().map(ToString::to_string))
        })
        .unwrap_or_else(|| json.to_string())
}

fn extract_port(addr: &str) -> Option<u16> {
    if let Some(stripped) = addr.strip_prefix('[') {
        let (_, port) = stripped.split_once(']')?;
        return port.strip_prefix(':')?.parse().ok();
    }

    let (_, port) = addr.rsplit_once(':')?;
    port.parse().ok()
}
