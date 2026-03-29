mod forward;
mod login;
mod types;

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::Instant;

use tracing::info;

use super::api::ApiService;
use super::config::{Config, SocketOptions};
use super::motd::{serve_legacy_ping, MotdService};
use super::outbound::connect_addr as connect_outbound_addr;
use super::players::PlayerRegistry;
use super::relay::relay_bidirectional;
use super::stats::ConnectionTraffic;
use super::traffic::{ConnectionCounters, TrafficReporter};
use crate::minecraft::{
    decode_handshake, encode_handshake, PacketIo, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE,
    MAX_LOGIN_PACKET_SIZE, PRISM_MAGIC_ID,
};

pub use types::{ConnectionContext, ConnectionReport, ConnectionRoute};

pub fn handle_client(
    mut client: std::net::TcpStream,
    config: &Config,
    api: &ApiService,
    motd: &MotdService,
    traffic_reporter: &TrafficReporter,
    players: &PlayerRegistry,
    context: ConnectionContext,
    started_at: Instant,
) -> io::Result<ConnectionReport> {
    client.set_read_timeout(Some(config.inbound.first_packet_timeout))?;

    let mut packet_io = PacketIo::new();
    let mut first_byte = [0_u8; 1];
    client.read_exact(&mut first_byte)?;
    if first_byte[0] == 0xFE {
        serve_legacy_ping(
            &mut client,
            &config.transport,
            config.relay.mode,
            players,
            context.id,
        )?;
        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            None,
            None,
        ));
    }

    packet_io.queue_slice(&first_byte);
    let handshake_packet = packet_io
        .read_frame(&mut client, MAX_HANDSHAKE_PACKET_SIZE)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if handshake_packet.frame.id == PRISM_MAGIC_ID {
        client.write_all(&"necron-prism".as_bytes().to_vec())?;
        client.shutdown(std::net::Shutdown::Both)?;
        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            None,
            None,
        ));
    }

    let handshake = decode_handshake(&handshake_packet)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    players.update_handshake(context.id, &handshake);

    info!(
        protocol_version = handshake.protocol_version,
        next_state = handshake.next_state,
        original_host = %handshake.server_address,
        original_port = handshake.server_port,
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "parsed client handshake"
    );

    if handshake.next_state == INTENT_STATUS {
        motd.serve(
            &mut packet_io,
            &mut client,
            &config.transport,
            config.relay.mode,
            &handshake,
            players,
            context.id,
        )
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            None,
            None,
        ));
    }

    let login_start_packet = packet_io
        .read_frame(&mut client, MAX_LOGIN_PACKET_SIZE)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let route = match login::resolve_login_route(
        &mut client,
        api,
        players,
        context.id,
        &login_start_packet,
        context.peer_addr,
    )? {
        Ok(route) => route,
        Err(report) => return Err(io::Error::other(HandledConnection(report))),
    };

    proxy_connection(
        client,
        config,
        traffic_reporter,
        players,
        context,
        handshake,
        handshake_packet,
        login_start_packet,
        route,
    )
}

fn proxy_connection(
    client: std::net::TcpStream,
    config: &Config,
    traffic_reporter: &TrafficReporter,
    players: &PlayerRegistry,
    context: ConnectionContext,
    mut handshake: crate::minecraft::HandshakeInfo,
    handshake_packet: crate::minecraft::FramedPacket,
    login_start_packet: crate::minecraft::FramedPacket,
    route: ConnectionRoute,
) -> io::Result<ConnectionReport> {
    let rewrite_addr = route.rewrite_addr.as_ref().unwrap_or(&route.target_addr);
    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    players.update_outbound(context.id, Arc::clone(&route.target_addr));

    let rewritten_packet =
        encode_handshake(&handshake).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    info!(
        rewrite_addr = %rewrite_addr,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %route.target_addr,
        "rewrote handshake and connecting outbound"
    );

    let socket_options: &SocketOptions = &config.inbound.socket_options;
    let mut upstream = connect_outbound_addr(route.target_addr.as_ref(), socket_options)?;
    let counters = ConnectionCounters::default();
    if let Ok(closer) = upstream.try_clone() {
        players.with_external_connection_id(context.id, |cid| {
            traffic_reporter.register(context.id, cid, counters.clone(), closer);
        });
    }

    upstream.write_all(&rewritten_packet)?;
    forward::forward_login_start(&mut upstream, &login_start_packet)?;

    let initial_upload_bytes =
        forward::compute_upload_bytes(&handshake_packet, &login_start_packet);
    counters.add_upload(initial_upload_bytes);

    let relay_stats = relay_bidirectional(client, upstream, counters.clone(), config.relay.mode)?;

    Ok(ConnectionReport::new(
        ConnectionTraffic {
            upload_bytes: initial_upload_bytes + relay_stats.upload_bytes,
            download_bytes: relay_stats.download_bytes,
        },
        relay_stats.mode,
        Some(route.target_addr),
        Some(route.rewrite_addr),
    ))
}

#[derive(Debug)]
pub(crate) struct HandledConnection(pub(crate) ConnectionReport);

impl std::fmt::Display for HandledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("connection already handled")
    }
}

impl std::error::Error for HandledConnection {}
