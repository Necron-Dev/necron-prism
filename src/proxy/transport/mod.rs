mod forward;
mod login;
mod types;

use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tracing::info;

use super::api::ApiService;
use super::config::Config;
use super::motd::{serve_legacy_ping, MotdService};
use super::outbound::connect_addr as connect_outbound_addr;
use super::players::PlayerRegistry;
use super::relay::relay_bidirectional;
use super::stats::ConnectionTraffic;
use super::traffic::TrafficReporter;
use crate::minecraft::{
    decode_handshake, encode_handshake, PacketIo, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE,
    MAX_LOGIN_PACKET_SIZE, PRISM_MAGIC_ID,
};

pub use types::{ConnectionContext, ConnectionReport, ConnectionRoute};

pub async fn handle_client(
    mut client: tokio::net::TcpStream,
    config: &Config,
    api: &ApiService,
    motd: &MotdService,
    traffic_reporter: &TrafficReporter,
    players: &PlayerRegistry,
    context: ConnectionContext,
    started_at: Instant,
) -> anyhow::Result<ConnectionReport> {
    info!(
        connection_id = context.id,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        phase = "start_handle_client",
        "starting client handling"
    );

    let mut packet_io = PacketIo::new();
    let mut first_byte = [0_u8; 1];
    timeout(
        config.inbound.first_packet_timeout,
        client.read_exact(&mut first_byte),
    )
    .await
    .with_context(|| {
        format!(
            "read first byte timed out after {}ms",
            config.inbound.first_packet_timeout.as_millis()
        )
    })?
    .context("read first byte")?;

    info!(
        connection_id = context.id,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        phase = "first_byte_read",
        first_byte = first_byte[0],
        "read first byte"
    );

    if first_byte[0] == 0xFE {
        serve_legacy_ping(
            &mut client,
            &config.transport,
            config.relay.mode,
            players,
            context.id,
        )
        .await
        .context("serve legacy ping")?;
        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            None,
            None,
        ));
    }

    packet_io.queue_slice(&first_byte);
    let handshake_packet = timeout(
        config.inbound.first_packet_timeout,
        packet_io.read_frame(&mut client, MAX_HANDSHAKE_PACKET_SIZE),
    )
    .await
    .with_context(|| {
        format!(
            "read handshake packet timed out after {}ms",
            config.inbound.first_packet_timeout.as_millis()
        )
    })?
    .context("read handshake packet")?;

    if handshake_packet.frame.id == PRISM_MAGIC_ID {
        client
            .write_all("necron-prism".as_bytes())
            .await
            .context("write magic response")?;
        client.shutdown().await.context("shutdown magic stream")?;
        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            None,
            None,
        ));
    }

    let handshake = decode_handshake(&handshake_packet)
        .map_err(anyhow::Error::from)
        .context("decode handshake")?;
    players.update_handshake(context.id, &handshake);

    info!(
        connection_id = context.id,
        protocol_version = handshake.protocol_version,
        next_state = handshake.next_state,
        original_host = %handshake.server_address,
        original_port = handshake.server_port,
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        phase = "handshake_decoded",
        "parsed client handshake"
    );

    if handshake.next_state == INTENT_STATUS {
        motd
            .serve(
                &mut packet_io,
                &mut client,
                &config.transport,
                config.relay.mode,
                &handshake,
                players,
                context.id,
            )
            .await
            .context("serve motd")?;

        return Ok(ConnectionReport::new(
            ConnectionTraffic::default(),
            None,
            None,
            None,
        ));
    }

    let login_start_packet = packet_io
        .read_frame(&mut client, MAX_LOGIN_PACKET_SIZE)
        .await
        .context("read login start packet")?;

    info!(
        connection_id = context.id,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        phase = "login_start_read",
        "read login start packet"
    );

    let route = match login::resolve_login_route(
        &mut client,
        api,
        players,
        context.id,
        &login_start_packet,
        context.peer_addr,
    )
    .await? {
        Ok(route) => route,
        Err(report) => return Err(anyhow::Error::new(HandledConnection(report))),
    };

    let counters = super::traffic::ConnectionCounters::default();

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
        started_at,
        counters.clone(),
    )
    .await
}

async fn proxy_connection(
    client: tokio::net::TcpStream,
    config: &Config,
    traffic_reporter: &TrafficReporter,
    players: &PlayerRegistry,
    context: ConnectionContext,
    mut handshake: crate::minecraft::HandshakeInfo,
    _handshake_packet: crate::minecraft::FramedPacket,
    login_start_packet: crate::minecraft::FramedPacket,
    route: ConnectionRoute,
    started_at: Instant,
    counters: super::traffic::ConnectionCounters,
) -> anyhow::Result<ConnectionReport> {
    let rewrite_addr = route
        .rewrite_addr
        .as_ref()
        .map(|a| a.as_ref())
        .unwrap_or(&route.target_addr);
    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|e| anyhow!(e))
        .context("rewrite handshake")?;
    players.update_outbound(context.id, Arc::clone(&route.target_addr));

    let rewritten_packet = encode_handshake(&handshake)
        .map_err(anyhow::Error::from)
        .context("encode handshake")?;

    info!(
        rewrite_addr = %rewrite_addr,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %route.target_addr,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        phase = "before_upstream_connect",
        "rewrote handshake and connecting outbound"
    );

    let mut upstream = connect_outbound_addr(route.target_addr.as_ref(), &config.inbound.socket_options)
        .await
        .context("connect upstream")?;

    if let Some(external_connection_id) =
        players.with_external_connection_id(context.id, |cid| cid.to_owned())
    {
        traffic_reporter.register(
            context.id,
            &external_connection_id,
            counters.clone(),
            None,
        );
    }

    upstream
        .write_all(&rewritten_packet)
        .await
        .context("write rewritten handshake")?;
    forward::forward_login_start(&mut upstream, &login_start_packet)
        .await
        .context("forward login start")?;

    let relay_stats = relay_bidirectional(
        client,
        upstream,
        counters.clone(),
        &config.inbound.socket_options,
        config.relay.mode,
    )
    .await
    .context("relay bidirectional")?;

    let upload_bytes = counters.upload();
    let download_bytes = counters.download();

    Ok(ConnectionReport::new(
        ConnectionTraffic {
            upload_bytes,
            download_bytes,
        },
        relay_stats.mode,
        Some(route.target_addr),
        route.rewrite_addr,
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
