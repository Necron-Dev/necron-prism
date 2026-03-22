use std::io::{self, Write};
use std::net::Shutdown;

use tracing::info;

use crate::minecraft::{FramedPacket, decode_login_hello, login_disconnect_packet};

use super::super::config::Config;
use super::super::players::{PlayerRegistry, PlayerState};
use super::super::stats::ConnectionTraffic;
use super::super::template;
use super::types::ConnectionReport;

pub fn handle_login_start(
    client: &mut std::net::TcpStream,
    config: &Config,
    players: &PlayerRegistry,
    connection_id: u64,
    handshake_packet: &FramedPacket,
    login_start_packet: &FramedPacket,
    outbound_name: &str,
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

    if let Some(kick_json) = &config.transport.kick_json {
        let rendered_kick = template::render(kick_json, players);
        let kick_packet = login_disconnect_packet(&rendered_kick)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        client.write_all(&kick_packet)?;
        client.shutdown(Shutdown::Both)?;
        players.set_state(connection_id, PlayerState::LoginRejectedLocally);

        info!(
            player_name = %login_hello.username,
            login_start_bytes = login_start_packet.wire_len,
            kick_packet_bytes = kick_packet.len(),
            "rejected login with local kick packet"
        );

        return Ok(Some(ConnectionReport::new(
            ConnectionTraffic {
                upload_bytes: (handshake_packet.wire_len + login_start_packet.wire_len) as u64,
                download_bytes: kick_packet.len() as u64,
            },
            None,
            Some(outbound_name.to_string()),
        )));
    }

    Ok(None)
}
