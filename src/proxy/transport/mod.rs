mod login;
mod types;

use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context as AnyhowContext};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tracing::{debug, info, trace};

use super::context::Context as ProxyContext;
use super::outbound::connect_addr as connect_outbound_addr;
use super::relay::relay_bidirectional;
use super::stats::ConnectionTraffic;
use crate::minecraft::{
    decode_handshake, encode_handshake, encode_raw_frame, PacketIo, INTENT_STATUS,
    MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE, PRISM_MAGIC_ID,
};

pub use types::{ConnectionContext, ConnectionReport, ConnectionRoute};

pub async fn handle_client(
    mut client: tokio::net::TcpStream,
    ctx: ProxyContext,
    conn: ConnectionContext,
) -> anyhow::Result<ConnectionReport> {
    let started_at = Instant::now();
    
    debug!(
        connection_id = conn.id,
        elapsed_ms = started_at.elapsed().as_millis(),
        phase = "start_handle_client",
        "connection handling started"
    );

    let config = ctx.config();
    let services = ctx.services();

    let mut packet_io = PacketIo::new();
    let mut first_byte = [0_u8; 1];
    timeout(
        config.first_packet_timeout(),
        client.read_exact(&mut first_byte),
    )
    .await
    .with_context(|| {
        format!(
            "read first byte timed out after {}ms",
            config.first_packet_timeout().as_millis()
        )
    })?
    .context("read first byte")?;

    trace!(
        connection_id = conn.id,
        elapsed_ms = started_at.elapsed().as_millis(),
        phase = "first_byte_read",
        first_byte = first_byte[0],
        "read first byte from client"
    );

    if first_byte[0] == 0xFE {
        info!(connection_id = conn.id, "detected legacy ping (0xFE)");
        super::motd::serve_legacy_ping(
            &mut client,
            &config.motd,
            config.relay_mode,
            &ctx.core.players,
            conn.id,
        )
        .await
        .context("serve legacy ping")?;
        return Ok(ConnectionReport::new(ConnectionTraffic::default(), None, None, None));
    }

    packet_io.queue_slice(&first_byte);
    let handshake_packet = timeout(
        config.first_packet_timeout(),
        packet_io.read_frame(&mut client, MAX_HANDSHAKE_PACKET_SIZE),
    )
    .await
    .with_context(|| {
        format!(
            "read handshake packet timed out after {}ms",
            config.first_packet_timeout().as_millis()
        )
    })?
    .context("read handshake packet")?;

    if handshake_packet.frame.id == PRISM_MAGIC_ID {
        client
            .write_all("necron-prism".as_bytes())
            .await
            .context("write magic response")?;
        client.shutdown().await.context("shutdown magic stream")?;
        return Ok(ConnectionReport::new(ConnectionTraffic::default(), None, None, None));
    }

    let handshake = decode_handshake(&handshake_packet)
        .map_err(anyhow::Error::from)
        .context("decode handshake")?;
    ctx.core.players.update_handshake(conn.id, &handshake);

    debug!(
        connection_id = conn.id,
        protocol_version = handshake.protocol_version,
        next_state = ?handshake.next_state,
        original_host = handshake.server_address,
        original_port = handshake.server_port,
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis(),
        phase = "handshake_decoded",
        "handshake packet decoded"
    );

    if handshake.next_state == INTENT_STATUS {
        services.motd
            .serve(
                &mut packet_io,
                &mut client,
                &config.motd,
                config.relay_mode,
                &handshake,
                &ctx.core.players,
                conn.id,
            )
            .await
            .context("serve motd")?;
        return Ok(ConnectionReport::new(ConnectionTraffic::default(), None, None, None));
    }

    let login_start_packet = packet_io
        .read_frame(&mut client, MAX_LOGIN_PACKET_SIZE)
        .await
        .context("read login start packet")?;

    debug!(
        connection_id = conn.id,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        phase = "login_start_read",
        "read login start packet"
    );

    let route = match login::resolve_login_route(
        &mut client,
        &services.api,
        &ctx.core.players,
        conn.id,
        &login_start_packet,
        conn.peer_addr,
        &config.api,
    )
    .await? {
        Ok(route) => route,
        Err(report) => return Err(anyhow::Error::new(HandledConnection(report))),
    };

    let counters = super::traffic::ConnectionCounters::default();
    proxy_connection(client, &ctx, conn, handshake, login_start_packet, route, started_at, counters.clone()).await
}

#[allow(clippy::too_many_arguments)]
async fn proxy_connection(
    client: tokio::net::TcpStream,
    ctx: &ProxyContext,
    conn: ConnectionContext,
    mut handshake: crate::minecraft::HandshakeInfo,
    login_start_packet: crate::minecraft::FramedPacket,
    route: ConnectionRoute,
    started_at: Instant,
    counters: super::traffic::ConnectionCounters,
) -> anyhow::Result<ConnectionReport> {
    let config = ctx.config();
    let services = ctx.services();

    let rewrite_addr = route
        .rewrite_addr
        .as_ref()
        .map(|a| a.as_ref())
        .unwrap_or(&route.target_addr);
    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|e| anyhow!(e))
        .context("rewrite handshake")?;
    ctx.core.players.update_outbound(conn.id, Arc::clone(&route.target_addr));

    let rewritten_packet = encode_handshake(&handshake)
        .map_err(anyhow::Error::from)
        .context("encode handshake")?;

    debug!(
        rewrite_addr = %rewrite_addr,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %route.target_addr,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        phase = "before_upstream_connect",
        "rewrote handshake and connecting outbound"
    );

    let mut upstream = connect_outbound_addr(route.target_addr.as_ref(), &config)
        .await
        .with_context(|| format!("failed to connect to upstream {}", route.target_addr))?;

    info!(connection_id = conn.id, target_addr = %route.target_addr, "upstream connected");

    if let Some(external_connection_id) =
        ctx.core.players.with_external_connection_id(conn.id, |cid| cid.to_owned())
    {
        services.traffic.register(conn.id, &external_connection_id, counters.clone(), None);
    }

    upstream
        .write_all(&rewritten_packet)
        .await
        .context("write rewritten handshake")?;
    let encoded_login_start = encode_raw_frame(&login_start_packet)
        .map_err(anyhow::Error::from)
        .context("encode login start")?;
    upstream.write_all(&encoded_login_start).await.context("write login start")?;

    let relay_stats = relay_bidirectional(client, upstream, counters.clone(), &config, config.relay_mode)
        .await
        .context("relay bidirectional")?;

    let upload_bytes = counters.upload();
    let download_bytes = counters.download();

    Ok(ConnectionReport::new(
        ConnectionTraffic { upload_bytes, download_bytes },
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
