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
    decode_handshake, encode_handshake, PacketIo, ProtocolError, INTENT_LOGIN, INTENT_STATUS,
    MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE,
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
        serve_legacy_ping(&mut client, &config.transport, players, context.id)?;
        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            Arc::<str>::from(""),
            Arc::<str>::from(""),
        ));
    }

    packet_io.queue_slice(&first_byte);
    let handshake_packet = packet_io
        .read_frame(&mut client, MAX_HANDSHAKE_PACKET_SIZE)
        .map_err(protocol_error)?;
    let handshake = decode_handshake(&handshake_packet).map_err(protocol_error)?;
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
            &handshake,
            players,
            context.id,
        )
        .map_err(protocol_error)?;

        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            Arc::<str>::from(""),
            Arc::<str>::from(""),
        ));
    }

    let login_start_packet = read_login_start_packet(&mut packet_io, &mut client, &handshake)?;
    let route = resolve_connection_route(
        &mut client,
        api,
        players,
        context,
        login_start_packet.as_ref(),
        config,
    )?;

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

fn read_login_start_packet(
    packet_io: &mut PacketIo,
    client: &mut std::net::TcpStream,
    handshake: &crate::minecraft::HandshakeInfo,
) -> io::Result<Option<crate::minecraft::FramedPacket>> {
    if handshake.next_state == INTENT_LOGIN {
        packet_io
            .read_frame(client, MAX_LOGIN_PACKET_SIZE)
            .map(Some)
            .map_err(protocol_error)
    } else {
        Ok(None)
    }
}

fn resolve_connection_route(
    client: &mut std::net::TcpStream,
    api: &ApiService,
    players: &PlayerRegistry,
    context: ConnectionContext,
    login_start_packet: Option<&crate::minecraft::FramedPacket>,
    config: &Config,
) -> io::Result<ConnectionRoute> {
    let Some(login_start_packet) = login_start_packet else {
        return Ok(ConnectionRoute {
            target_addr: Arc::<str>::from(config.api.mock.target_addr.as_str()),
            rewrite_addr: Arc::<str>::from(config.api.mock.target_addr.as_str()),
        });
    };

    match login::resolve_login_route(
        client,
        api,
        players,
        context.id,
        login_start_packet,
        context.peer_addr,
    )? {
        Ok(route) => Ok(route),
        Err(report) => Err(io::Error::other(HandledConnection(report))),
    }
}

fn proxy_connection(
    client: std::net::TcpStream,
    config: &Config,
    traffic_reporter: &TrafficReporter,
    players: &PlayerRegistry,
    context: ConnectionContext,
    mut handshake: crate::minecraft::HandshakeInfo,
    handshake_packet: crate::minecraft::FramedPacket,
    login_start_packet: Option<crate::minecraft::FramedPacket>,
    route: ConnectionRoute,
) -> io::Result<ConnectionReport> {
    handshake
        .rewrite_addr(route.rewrite_addr.as_ref())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    players.update_outbound(context.id, route.target_addr.to_string());

    let rewritten_packet = encode_handshake(&handshake).map_err(protocol_error)?;

    info!(
        rewrite_addr = %route.rewrite_addr,
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
    forward::forward_login_start(&mut upstream, login_start_packet.as_ref())?;

    let initial_upload_bytes =
        forward::compute_upload_bytes(&handshake_packet, login_start_packet.as_ref());
    counters.add_upload(initial_upload_bytes);

    let relay_stats = relay_bidirectional(client, upstream, counters.clone(), config.relay.mode)?;
    let connection_traffic = ConnectionTraffic {
        upload_bytes: initial_upload_bytes + relay_stats.upload_bytes,
        download_bytes: relay_stats.download_bytes,
    };

    Ok(ConnectionReport::new(
        connection_traffic,
        relay_stats.mode,
        route.target_addr,
        route.rewrite_addr,
    ))
}

fn protocol_error(error: ProtocolError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[derive(Debug)]
pub(crate) struct HandledConnection(pub(crate) ConnectionReport);

impl std::fmt::Display for HandledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("connection already handled")
    }
}

impl std::error::Error for HandledConnection {}
