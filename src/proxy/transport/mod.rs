mod forward;
mod login;
mod types;

use std::io::{self, Read, Write};
use std::time::Instant;

use tracing::info;

use super::api::ApiService;
use crate::minecraft::{
    INTENT_LOGIN, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE, PacketIo,
    ProtocolError, decode_handshake, encode_handshake,
};

use super::config::Config;
use super::motd::{MotdService, serve_legacy_ping};
use super::outbound::{connect_addr as connect_outbound_addr, fallback_outbound, select_outbound};
use super::players::PlayerRegistry;
use super::relay::relay_bidirectional;
use super::stats::ConnectionTraffic;
use super::traffic::{ConnectionCounters, TrafficReporter};

pub use types::{ConnectionContext, ConnectionReport};

pub fn handle_client(
    mut client: std::net::TcpStream,
    config: &Config,
    api: Option<&ApiService>,
    traffic_reporter: Option<&TrafficReporter>,
    players: &PlayerRegistry,
    context: ConnectionContext,
    started_at: Instant,
) -> io::Result<ConnectionReport> {
    client.set_read_timeout(Some(config.inbound.first_packet_timeout))?;

    let mut packet_io = PacketIo::new();
    let mut first_byte = [0_u8; 1];
    client.read_exact(&mut first_byte)?;
    if first_byte[0] == 0xFE {
        let outbound = fallback_outbound(config);
        let traffic = serve_legacy_ping(
            &mut client,
            &config.transport,
            outbound,
            players,
            context.id,
        )?;

        return Ok(ConnectionReport::new(
            traffic,
            None,
            Some(outbound.name.clone()),
            None,
        ));
    }

    packet_io.queue_slice(&first_byte);
    let handshake_packet = packet_io
        .read_frame(&mut client, MAX_HANDSHAKE_PACKET_SIZE)
        .map_err(protocol_error)?;
    let mut handshake = decode_handshake(&handshake_packet).map_err(protocol_error)?;
    players.update_handshake(context.id, &handshake);

    let outbound = select_outbound(config, &handshake);
    let mut api_target_override = None;
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
            None,
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
            api,
            players,
            context.id,
            &handshake_packet,
            login_start_packet,
            outbound,
            &mut api_target_override,
            context.peer_addr,
        )? {
            return Ok(report);
        }
    }

    let target_addr = api_target_override
        .as_deref()
        .unwrap_or(&outbound.target_addr);
    let rewrite_addr = api_target_override
        .as_deref()
        .unwrap_or(&outbound.rewrite_addr);

    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    players.update_outbound(context.id, outbound.name.clone());

    let rewritten_packet = encode_handshake(&handshake).map_err(protocol_error)?;

    info!(
        selected_outbound = %outbound.name,
        rewrite_addr = %rewrite_addr,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %target_addr,
        "rewrote handshake and connecting outbound"
    );

    client.set_read_timeout(None)?;

    let mut upstream = connect_outbound_addr(outbound, target_addr)?;
    let counters = ConnectionCounters::default();
    if let (Some(reporter), Some(cid), Ok(closer)) = (
        traffic_reporter,
        players.external_connection_id(context.id),
        upstream.try_clone(),
    ) {
        reporter.register(context.id, cid, counters.clone(), closer);
    }
    upstream.write_all(&rewritten_packet)?;
    forward::forward_login_start(&mut upstream, login_start_packet.as_ref())?;

    counters.add_upload(forward::compute_upload_bytes(
        &handshake_packet,
        login_start_packet.as_ref(),
    ));

    let relay_stats = relay_bidirectional(client, upstream, config.relay.mode)?;
    counters.add_upload(relay_stats.upload_bytes);
    counters.add_download(relay_stats.download_bytes);
    let traffic = ConnectionTraffic {
        upload_bytes: forward::compute_upload_bytes(&handshake_packet, login_start_packet.as_ref())
            + relay_stats.upload_bytes,
        download_bytes: relay_stats.download_bytes,
    };

    Ok(ConnectionReport::new(
        traffic,
        relay_stats.mode,
        Some(outbound.name.clone()),
        players.external_connection_id(context.id),
    ))
}

fn protocol_error(error: ProtocolError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
