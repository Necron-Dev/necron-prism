mod login;
pub(crate) mod relay;

use std::time::Instant;

use anyhow::{anyhow, Context as AnyhowContext};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tracing::{debug, info, trace};
use std::time::Duration;

use super::context::Context as ProxyContext;
use super::outbound::connect_addr as connect_outbound_addr;
use super::stats::{ConnectionReport, ConnectionRoute, ConnectionSession, ConnectionTraffic};
use self::relay::relay_bidirectional;
use crate::minecraft::{
    decode_handshake, encode_handshake, encode_raw_frame, PacketIo, INTENT_STATUS,
    MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE, PRISM_MAGIC_ID,
};

pub async fn handle_client(
    mut client: tokio::net::TcpStream,
    ctx: ProxyContext,
    session: ConnectionSession,
) -> anyhow::Result<ConnectionReport> {
    let logging_conn = session.clone();
    let _guard = logging_conn.enter_stage("CONNECT/TRANSPORT");
    let started_at = Instant::now();
    
    debug!(
        elapsed_ms = started_at.elapsed().as_millis(),
        "[CONNECT/TRANSPORT] connection handling started"
    );

    let config = ctx.config();
    let services = ctx.services();
    let first_packet_timeout = Duration::from_millis(config.network.socket.first_packet_timeout_ms);

    let mut packet_io = PacketIo::new();
    let mut first_byte = [0_u8; 1];
    timeout(
        first_packet_timeout,
        client.read_exact(&mut first_byte),
    )
    .await
    .with_context(|| {
        format!(
            "read first byte timed out after {}ms",
            first_packet_timeout.as_millis()
        )
    })?
    .context("read first byte")?;

    trace!(
        elapsed_ms = started_at.elapsed().as_millis(),
        first_byte = first_byte[0],
        "[CONNECT/TRANSPORT] read first byte from client"
    );

    if first_byte[0] == 0xFE {
        session.record_stage("MOTD");
        info!(connection_id = session.id, "[MOTD] detected legacy ping (0xFE)");
        super::motd::serve_legacy_ping(&mut client, &ctx, session.id)
        .await
        .context("serve legacy ping")?;
        return Ok(ConnectionReport::new(ConnectionTraffic::default(), None, None, None));
    }

    packet_io.queue_slice(&first_byte);
    let handshake_packet = timeout(
        first_packet_timeout,
        packet_io.read_frame(&mut client, MAX_HANDSHAKE_PACKET_SIZE),
    )
    .await
    .with_context(|| {
        format!(
            "read handshake packet timed out after {}ms",
            first_packet_timeout.as_millis()
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
    ctx.core.players.update_handshake(session.id, &handshake);

    debug!(
        protocol_version = handshake.protocol_version,
        next_state = ?handshake.next_state,
        original_host = handshake.server_address,
        original_port = handshake.server_port,
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis(),
        "[CONNECT/TRANSPORT] handshake packet decoded"
    );

    if handshake.next_state == INTENT_STATUS {
        session.record_stage("MOTD");
        services.motd
            .serve(
                &mut packet_io,
                &mut client,
                &ctx,
                &handshake,
                &session,
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
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "[CONNECT/TRANSPORT] read login start packet"
    );

    let route = match login::resolve_login_route(
        &mut client,
        &ctx,
        &session,
        &login_start_packet,
        session.peer_addr,
    )
    .await? {
        Ok(route) => route,
        Err(report) => return Err(anyhow::Error::new(HandledConnection(report))),
    };

    proxy_connection(client, &ctx, session, handshake, login_start_packet, route, started_at).await
}

#[allow(clippy::too_many_arguments)]
async fn proxy_connection(
    client: tokio::net::TcpStream,
    ctx: &ProxyContext,
    session: ConnectionSession,
    mut handshake: crate::minecraft::HandshakeInfo,
    login_start_packet: crate::minecraft::FramedPacket,
    route: ConnectionRoute,
    started_at: Instant,
) -> anyhow::Result<ConnectionReport> {
    let _guard = session.enter_stage("CONNECT/OUTBOUND");
    let config = ctx.config();
    let services = ctx.services();

    let rewrite_addr = route
        .rewrite_addr
        .as_ref()
        .unwrap_or(&route.target_addr);
    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|e| anyhow!(e))
        .context("rewrite handshake")?;
    ctx.core
        .players
        .update_outbound(session.id, route.target_addr.as_str().into());

    let rewritten_packet = encode_handshake(&handshake)
        .map_err(anyhow::Error::from)
        .context("encode handshake")?;

    debug!(
        rewrite_addr = %rewrite_addr,
        rewritten_handshake_bytes = rewritten_packet.len(),
        target_addr = %route.target_addr,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "[CONNECT/OUTBOUND] rewrote handshake and connecting upstream"
    );

    let mut upstream = connect_outbound_addr(&route.target_addr, &config, &session)
        .await
        .with_context(|| format!("failed to connect to upstream {}", route.target_addr))?;

    info!(target_addr = %route.target_addr, "[CONNECT/OUTBOUND] upstream connected");

    if let Some(external_connection_id) =
        ctx.core.players.with_external_connection_id(session.id, |cid| cid.to_owned())
    {
        services
            .traffic
            .register(session.id, &external_connection_id, session.clone(), None);
    }

    upstream
        .write_all(&rewritten_packet)
        .await
        .context("write rewritten handshake")?;
    let encoded_login_start = encode_raw_frame(&login_start_packet)
        .map_err(anyhow::Error::from)
        .context("encode login start")?;
    upstream.write_all(&encoded_login_start).await.context("write login start")?;

    let relay_stats = relay_bidirectional(client, upstream, session.clone(), &config)
        .await
        .context("relay bidirectional")?;

    Ok(ConnectionReport::new(
        session.connection_traffic(),
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
