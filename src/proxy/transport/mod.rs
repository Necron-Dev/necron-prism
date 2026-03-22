use std::io::{self, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Instant;

use tracing::info;

use crate::minecraft::{
    INTENT_LOGIN, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE, PacketIo,
    ProtocolError, decode_handshake, decode_login_hello, encode_handshake, encode_raw_frame,
    login_disconnect_packet,
};

use super::config::Config;
use super::motd::MotdService;
use super::outbound::{SelectedOutbound, connect as connect_outbound, select_outbound};
use super::players::{PlayerRegistry, PlayerState};
use super::relay::{RelayMode, relay_bidirectional};
use super::stats::ConnectionTraffic;
use super::template;

#[derive(Clone, Copy, Debug)]
pub struct ConnectionContext {
    pub id: u64,
    pub peer_addr: Option<SocketAddr>,
}

#[derive(Clone, Debug)]
pub struct ConnectionReport {
    pub traffic: ConnectionTraffic,
    pub relay_mode: Option<RelayMode>,
    pub outbound_name: Option<String>,
}

pub fn handle_client(
    mut client: TcpStream,
    config: &Config,
    players: &PlayerRegistry,
    context: ConnectionContext,
    started_at: Instant,
) -> io::Result<ConnectionReport> {
    client.set_read_timeout(Some(config.inbound.first_packet_timeout))?;

    let mut packet_io = PacketIo::new();
    let handshake_packet = packet_io
        .read_frame(&mut client, MAX_HANDSHAKE_PACKET_SIZE)
        .map_err(protocol_error)?;
    let mut handshake = decode_handshake(&handshake_packet).map_err(protocol_error)?;
    players.update_handshake(context.id, &handshake, Instant::now());

    let selected = select_outbound(config, &handshake);
    let selected_for_player = SelectedOutbound::from(selected);
    let motd = MotdService::default();

    info!(
        protocol_version = handshake.protocol_version,
        next_state = handshake.next_state,
        original_host = %handshake.server_address,
        original_port = handshake.server_port,
        selected_outbound = %selected.name,
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "parsed client handshake"
    );

    if handshake.next_state == INTENT_STATUS {
        let traffic = motd
            .serve(
                &mut packet_io,
                &mut client,
                &config.transport,
                selected,
                &handshake,
                handshake_packet.wire_len,
                players,
                context.id,
            )
            .map_err(protocol_error)?;

        return Ok(ConnectionReport {
            traffic,
            relay_mode: None,
            outbound_name: Some(selected.name.clone()),
        });
    }

    let login_start_packet = if handshake.next_state == INTENT_LOGIN {
        Some(
            packet_io
                .read_frame(&mut client, MAX_LOGIN_PACKET_SIZE)
                .map_err(protocol_error)?,
        )
    } else {
        None
    };

    if let Some(login_start_packet) = login_start_packet.as_ref() {
        let login_hello = decode_login_hello(login_start_packet).map_err(protocol_error)?;
        players.update_username(context.id, login_hello.username.clone());
        if let Some(uuid) = login_hello.profile_id {
            players.update_uuid(context.id, uuid);
        } else {
            players.clear_uuid(context.id);
        }

        info!(
            player_name = %login_hello.username,
            player_uuid = ?login_hello.profile_id,
            login_start_bytes = login_start_packet.wire_len,
            "parsed login hello"
        );

        if let Some(kick_json) = &config.transport.kick_json {
            let traffic = handle_login_kick(
                &mut client,
                kick_json,
                handshake_packet.wire_len,
                login_start_packet.wire_len,
                &login_hello.username,
                players,
                context.id,
            )?;

            return Ok(ConnectionReport {
                traffic,
                relay_mode: None,
                outbound_name: Some(selected.name.clone()),
            });
        }
    }

    handshake
        .rewrite_addr(&selected.rewrite_addr)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    players.update_outbound(context.id, selected_for_player);

    let rewritten_packet = encode_handshake(&handshake).map_err(protocol_error)?;

    info!(
        selected_outbound = %selected.name,
        rewrite_addr = %selected.rewrite_addr,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %selected.target_addr,
        "rewrote handshake and connecting outbound"
    );

    client.set_read_timeout(None)?;

    let mut upstream = connect_outbound(selected)?;
    upstream.write_all(&rewritten_packet)?;
    if let Some(login_start_packet) = login_start_packet.as_ref() {
        let encoded_login_start = encode_raw_frame(login_start_packet).map_err(protocol_error)?;
        upstream.write_all(&encoded_login_start)?;
    }

    let relay_stats = relay_bidirectional(client, upstream)?;
    let traffic = ConnectionTraffic {
        upload_bytes: relay_stats.upload_bytes
            + handshake_packet.wire_len as u64
            + login_start_packet
                .as_ref()
                .map(|packet| packet.wire_len as u64)
                .unwrap_or(0),
        download_bytes: relay_stats.download_bytes,
    };

    Ok(ConnectionReport {
        traffic,
        relay_mode: relay_stats.mode,
        outbound_name: Some(selected.name.clone()),
    })
}

fn handle_login_kick(
    client: &mut TcpStream,
    kick_json: &str,
    handshake_wire_bytes: usize,
    login_start_wire_bytes: usize,
    player_name: &str,
    players: &PlayerRegistry,
    connection_id: u64,
) -> io::Result<ConnectionTraffic> {
    let rendered_kick = template::render(kick_json, players);
    let kick_packet = login_disconnect_packet(&rendered_kick).map_err(protocol_error)?;
    client.write_all(&kick_packet)?;
    client.shutdown(Shutdown::Both)?;
    players.set_state(
        connection_id,
        PlayerState::LoginRejectedLocally,
        Instant::now(),
    );

    info!(
        player_name = %player_name,
        login_start_bytes = login_start_wire_bytes,
        kick_packet_bytes = kick_packet.len(),
        "rejected login with local kick packet"
    );

    Ok(ConnectionTraffic {
        upload_bytes: (handshake_wire_bytes + login_start_wire_bytes) as u64,
        download_bytes: kick_packet.len() as u64,
    })
}

fn protocol_error(error: ProtocolError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
