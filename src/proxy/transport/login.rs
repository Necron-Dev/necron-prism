use std::io::{self, Write};
use std::net::{Shutdown, SocketAddr};

use tracing::info;

use crate::minecraft::{FramedPacket, decode_login_hello, login_disconnect_packet};

use super::super::api::{ApiService, JoinDecision};
use super::super::config::Config;
use super::super::players::{PlayerRegistry, PlayerState};
use super::super::stats::ConnectionTraffic;
use super::super::template;
use super::types::ConnectionReport;

pub fn handle_login_start(
    client: &mut std::net::TcpStream,
    config: &Config,
    api: Option<&ApiService>,
    players: &PlayerRegistry,
    connection_id: u64,
    handshake_packet: &FramedPacket,
    login_start_packet: &FramedPacket,
    outbound: &crate::proxy::config::OutboundConfig,
    api_target_override: &mut Option<String>,
    peer_addr: Option<SocketAddr>,
) -> io::Result<Option<ConnectionReport>> {
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

    let mut external_connection_id = None;
    if let Some(api) = api {
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
            Ok(JoinDecision::Allow {
                server_ip,
                connection_id: external_id,
            }) => {
                players.update_external_connection_id(connection_id, external_id.clone());
                *api_target_override = Some(server_ip);
                external_connection_id = Some(external_id);
            }
            Ok(JoinDecision::Deny { kick_reason }) => {
                return deny_with_reason(
                    client,
                    &kick_reason,
                    players,
                    connection_id,
                    handshake_packet,
                    login_start_packet,
                    outbound.name.as_str(),
                    external_connection_id,
                );
            }
            Err(error) => return Err(io::Error::other(error)),
        }
    }

    if let Some(kick_json) = &config.transport.kick_json {
        let rendered_kick = template::render(kick_json, players);
        return deny_with_reason(
            client,
            &rendered_kick,
            players,
            connection_id,
            handshake_packet,
            login_start_packet,
            outbound.name.as_str(),
            external_connection_id,
        );
    }

    Ok(None)
}

fn deny_with_reason(
    client: &mut std::net::TcpStream,
    reason: &str,
    players: &PlayerRegistry,
    connection_id: u64,
    handshake_packet: &FramedPacket,
    login_start_packet: &FramedPacket,
    outbound_name: &str,
    external_connection_id: Option<String>,
) -> io::Result<Option<ConnectionReport>> {
    let kick_packet = login_disconnect_packet(reason)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    client.write_all(&kick_packet)?;
    client.shutdown(Shutdown::Both)?;
    players.set_state(connection_id, PlayerState::LoginRejectedLocally);

    info!(
        login_start_bytes = login_start_packet.wire_len,
        kick_packet_bytes = kick_packet.len(),
        "rejected login with local kick packet"
    );

    Ok(Some(ConnectionReport::new(
        ConnectionTraffic {
            upload_bytes: (handshake_packet.wire_len + login_start_packet.wire_len) as u64,
            download_bytes: kick_packet.len() as u64,
        },
        None,
        Some(outbound_name.to_string()),
        external_connection_id,
    )))
}
