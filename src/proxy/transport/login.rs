use std::io::{self, Write};
use std::net::{Shutdown, SocketAddr};

use tracing::info;

use crate::minecraft::{decode_login_hello, login_disconnect_packet, FramedPacket};

use super::super::api::{ApiService, JoinDecision};
use super::super::players::{PlayerRegistry, PlayerState};
use super::super::stats::ConnectionTraffic;
use super::types::{ConnectionReport, ConnectionRoute};

pub fn resolve_login_route(
    client: &mut std::net::TcpStream,
    api: &ApiService,
    players: &PlayerRegistry,
    connection_id: u64,
    handshake_packet: &FramedPacket,
    login_start_packet: &FramedPacket,
    peer_addr: Option<SocketAddr>,
) -> io::Result<Result<ConnectionRoute, ConnectionReport>> {
    let login_hello = decode_login_hello(login_start_packet)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    players.update_login(
        connection_id,
        login_hello.username.clone(),
        login_hello.profile_id,
    );

    info!(
        player_name = %login_hello.username,
        player_uuid = ?login_hello.profile_id,
        login_start_bytes = login_start_packet.wire_len,
        "parsed login hello"
    );

    match api.join(
        Some(&login_hello.username),
        login_hello
            .profile_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        peer_addr.as_ref().map(ToString::to_string).as_deref(),
        players.current_online_count(),
    ) {
        Ok(JoinDecision::Allow(target)) => {
            players.update_external_connection_id(connection_id, target.connection_id.clone());
            Ok(Ok(ConnectionRoute {
                target_addr: target.target_addr,
                rewrite_addr: target.rewrite_addr,
            }))
        }
        Ok(JoinDecision::Deny { kick_reason }) => deny_with_reason(
            client,
            &kick_reason,
            players,
            connection_id,
            handshake_packet,
            login_start_packet,
        )
        .map(Err),
        Err(error) => Err(io::Error::other(error)),
    }
}

fn deny_with_reason(
    client: &mut std::net::TcpStream,
    reason: &str,
    players: &PlayerRegistry,
    connection_id: u64,
    handshake_packet: &FramedPacket,
    login_start_packet: &FramedPacket,
) -> io::Result<ConnectionReport> {
    let kick_packet = login_disconnect_packet(reason)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    client.write_all(&kick_packet)?;
    client.shutdown(Shutdown::Both)?;
    players.set_state(connection_id, PlayerState::LoginRejectedLocally);

    info!(
        login_start_bytes = login_start_packet.wire_len,
        kick_packet_bytes = kick_packet.len(),
        "rejected login with api kick packet"
    );

    Ok(ConnectionReport::new(
        ConnectionTraffic {
            upload_bytes: (handshake_packet.wire_len + login_start_packet.wire_len) as u64,
            download_bytes: kick_packet.len() as u64,
        },
        None,
        String::new(),
        String::new(),
    ))
}
