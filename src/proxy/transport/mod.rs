mod forward;
mod login;
mod types;

use std::io::{self, Write};
use std::time::Instant;

use tracing::info;

use crate::minecraft::{
    INTENT_LOGIN, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE, PacketIo,
    ProtocolError, decode_handshake, encode_handshake,
};

use super::config::Config;
use super::motd::MotdService;
use super::outbound::{connect as connect_outbound, select_outbound};
use super::players::PlayerRegistry;
use super::relay::relay_bidirectional;
use super::stats::ConnectionTraffic;

pub use types::{ConnectionContext, ConnectionReport};

pub fn handle_client(
    mut client: std::net::TcpStream,
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
    players.update_handshake(context.id, &handshake);

    let outbound = select_outbound(config, &handshake);
    let motd = MotdService::default();

    info!(
        protocol_version = handshake.protocol_version,
        next_state = handshake.next_state,
        original_host = %handshake.server_address,
        original_port = handshake.server_port,
        selected_outbound = %outbound.name,
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
                outbound,
                &handshake,
                handshake_packet.wire_len,
                players,
                context.id,
            )
            .map_err(protocol_error)?;

        return Ok(ConnectionReport::new(
            traffic,
            None,
            Some(outbound.name.clone()),
        ));
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
        if let Some(report) = login::handle_login_start(
            &mut client,
            config,
            players,
            context.id,
            &handshake_packet,
            login_start_packet,
            outbound.name.as_str(),
        )? {
            return Ok(report);
        }
    }

    handshake
        .rewrite_addr(&outbound.rewrite_addr)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    players.update_outbound(context.id, outbound.name.clone());

    let rewritten_packet = encode_handshake(&handshake).map_err(protocol_error)?;

    info!(
        selected_outbound = %outbound.name,
        rewrite_addr = %outbound.rewrite_addr,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %outbound.target_addr,
        "rewrote handshake and connecting outbound"
    );

    client.set_read_timeout(None)?;

    let mut upstream = connect_outbound(outbound)?;
    upstream.write_all(&rewritten_packet)?;
    forward::forward_login_start(&mut upstream, login_start_packet.as_ref())?;

    let relay_stats = relay_bidirectional(client, upstream, config.relay.mode)?;
    let traffic = ConnectionTraffic {
        upload_bytes: forward::compute_upload_bytes(&handshake_packet, login_start_packet.as_ref())
            + relay_stats.upload_bytes,
        download_bytes: relay_stats.download_bytes,
    };

    Ok(ConnectionReport::new(
        traffic,
        relay_stats.mode,
        Some(outbound.name.clone()),
    ))
}

fn protocol_error(error: ProtocolError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
