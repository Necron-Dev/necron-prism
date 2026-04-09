use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as AnyhowContext};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tracing::{debug, info, trace, warn};

use necron_prism_minecraft::{
    decode_handshake, encode_handshake, encode_raw_frame,
    PacketIo, INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE,
    MAX_LOGIN_PACKET_SIZE, PRISM_MAGIC_ID,
};

use crate::context::PrismContext;
use crate::hooks::{LoginResult, PrismHooks};
use crate::outbound::connect_addr as connect_outbound_addr;
use crate::players::PlayerState;
use crate::relay::relay_bidirectional;
use crate::session::{ConnectionKind, ConnectionReport, ConnectionRoute, ConnectionSession, ConnectionTraffic};

pub async fn handle_connection<H: PrismHooks>(
    ctx: PrismContext<H>,
    client: tokio::net::TcpStream,
    session: ConnectionSession,
) {
    let started_at = Instant::now();

    match handle_client(client, &ctx, &session).await {
        Ok(report) => finish_success(&ctx, &session, started_at, report),
        Err(error) => {
            if let Some(report) = error.downcast_ref::<HandledConnection>().map(|h| &h.0) {
                finish_success(&ctx, &session, started_at, report.clone());
            } else {
                let traffic = session.connection_traffic();
                let report = ConnectionReport::new(traffic, None, None, None);
                ctx.hooks().on_connection_finished(&session, &report);
                let _settled = ctx.runtime().totals.record_finished_connection(traffic);
                let remaining = ctx.runtime().players.remove_connection(session.id);
                let tag = session.kind().tag();
                let is_expected_disconnect = is_expected_disconnect(&error);
                let is_motd = session.kind() == ConnectionKind::Motd;
                if is_expected_disconnect {
                    let level = if is_motd { tracing::Level::DEBUG } else { tracing::Level::INFO };
                    tracing::event!(
                        level,
                        connection_id = session.id,
                        error = %error,
                        elapsed_ms = started_at.elapsed().as_millis() as u64,
                        upload_bytes = traffic.upload_bytes,
                        download_bytes = traffic.download_bytes,
                        active_remaining = remaining,
                        "[{tag}] connection closed"
                    );
                } else {
                    warn!(
                        connection_id = session.id,
                        error = %error,
                        elapsed_ms = started_at.elapsed().as_millis() as u64,
                        upload_bytes = traffic.upload_bytes,
                        download_bytes = traffic.download_bytes,
                        active_remaining = remaining,
                        "[{tag}] connection closed"
                    );
                } else {
                    warn!(
                        connection_id = session.id,
                        error = %error,
                        elapsed_ms = started_at.elapsed().as_millis() as u64,
                        upload_bytes = traffic.upload_bytes,
                        download_bytes = traffic.download_bytes,
                        active_remaining = remaining,
                        "[{tag}] connection failed"
                    );
                }
            }
        }
    }
}

fn is_expected_disconnect(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            matches!(
                io_err.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            )
        } else {
            false
        }
    })
}

async fn handle_client<H: PrismHooks>(
    mut client: tokio::net::TcpStream,
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
) -> anyhow::Result<ConnectionReport> {
    let started_at = Instant::now();

    debug!(
        elapsed_ms = started_at.elapsed().as_millis(),
        "[CONNECT] connection handling started"
    );

    let config = ctx.config();
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
        "[CONNECT] read first byte from client"
    );

    if first_byte[0] == 0xFE {
        session.set_kind(ConnectionKind::Motd);
        let _motd_guard = session.enter_stage("CONNECT/MOTD");
        info!(connection_id = session.id, "[CONNECT/MOTD] detected legacy ping (0xFE)");
        let online_count = ctx.runtime().players.current_online_count();
        let config = ctx.config();
        ctx.hooks()
            .on_legacy_ping(&mut client, session, &config, online_count)
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
    ctx.runtime().players.update_handshake(session.id, &handshake);

    debug!(
        protocol_version = handshake.protocol_version,
        next_state = ?handshake.next_state,
        original_host = handshake.server_address,
        original_port = handshake.server_port,
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis(),
        "[CONNECT] handshake packet decoded"
    );

    if handshake.next_state == INTENT_STATUS {
        session.set_kind(ConnectionKind::Motd);
        let _motd_guard = session.enter_stage("CONNECT/MOTD");
        let online_count = ctx.runtime().players.current_online_count();
        let config = ctx.config();
        ctx.hooks()
            .on_status_request(&mut packet_io, &mut client, session, &handshake, &config, online_count)
            .await
            .context("serve motd")?;
        return Ok(ConnectionReport::new(ConnectionTraffic::default(), None, None, None));
    }

    session.set_kind(ConnectionKind::Proxy);

    let login_start_packet = packet_io
        .read_frame(&mut client, MAX_LOGIN_PACKET_SIZE)
        .await
        .context("read login start packet")?;

    debug!(
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "[CONNECT/LOGIN] read login start packet"
    );

    let online_count = ctx.runtime().players.current_online_count();
    let config = ctx.config();
    let login_result = ctx
        .hooks()
        .on_login(&mut client, session, &login_start_packet, session.peer_addr, &config, online_count)
        .await?;

    let route = match login_result {
        LoginResult::Allow(route) => route,
        LoginResult::Deny { kick_reason } => {
            info!(
                kick_reason = %kick_reason,
                "[CONNECT/LOGIN] player join denied"
            );
            let kick_packet = necron_prism_minecraft::login_disconnect_packet(&kick_reason)
                .map_err(anyhow::Error::from)
                .context("build disconnect packet")?;
            client.write_all(&kick_packet).await?;
            client.shutdown().await?;
            ctx.runtime().players.set_state(session.id, PlayerState::LoginRejectedLocally);

            debug!(
                login_start_bytes = login_start_packet.wire_len,
                kick_packet_bytes = kick_packet.len(),
                "[CONNECT/LOGIN] rejected login with kick packet"
            );

            return Err(anyhow::Error::new(HandledConnection(
                ConnectionReport::new(ConnectionTraffic::default(), None, None, None),
            )));
        }
    };

    proxy_connection(client, ctx, session, handshake, login_start_packet, route, started_at).await
}

#[allow(clippy::too_many_arguments)]
async fn proxy_connection<H: PrismHooks>(
    client: tokio::net::TcpStream,
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
    mut handshake: necron_prism_minecraft::HandshakeInfo,
    login_start_packet: necron_prism_minecraft::FramedPacket,
    route: ConnectionRoute,
    started_at: Instant,
) -> anyhow::Result<ConnectionReport> {
    let _guard = session.enter_stage("CONNECT/OUTBOUND");
    let config = ctx.config();

    let rewrite_addr = route
        .rewrite_addr
        .as_ref()
        .unwrap_or(&route.target_addr);
    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|e| anyhow!(e))
        .context("rewrite handshake")?;
    ctx.runtime()
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

    let mut upstream = connect_outbound_addr(&route.target_addr, &config, session)
        .await
        .with_context(|| format!("failed to connect to upstream {}", route.target_addr))?;

    info!(target_addr = %route.target_addr, "[CONNECT/OUTBOUND] upstream connected");

    if let Some(external_connection_id) = &route.external_connection_id {
        ctx.hooks()
            .on_connection_established(session, external_connection_id);
    }

    let encoded_login_start = encode_raw_frame(&login_start_packet)
        .map_err(anyhow::Error::from)
        .context("encode login start")?;

    // Coalesce handshake + login-start into a single write to reduce syscall count
    let mut combined = Vec::with_capacity(rewritten_packet.len() + encoded_login_start.len());
    combined.extend_from_slice(&rewritten_packet);
    combined.extend_from_slice(&encoded_login_start);
    upstream
        .write_all(&combined)
        .await
        .context("write rewritten handshake + login start")?;

    let relay_stats = relay_bidirectional(client, upstream, session.clone(), &config)
        .await
        .context("relay bidirectional")?;

    let report = ConnectionReport::new(
        session.connection_traffic(),
        relay_stats.mode,
        Some(route.target_addr),
        route.rewrite_addr,
    );

    ctx.hooks().on_connection_finished(session, &report);

    Ok(report)
}

fn finish_success<H: PrismHooks>(ctx: &PrismContext<H>, session: &ConnectionSession, started_at: Instant, report: ConnectionReport) {
    let _settled = ctx.runtime().totals.record_finished_connection(report.connection_traffic);
    let remaining = ctx.runtime().players.remove_connection(session.id);
    let tag = session.kind().tag();

    if let Some(mode) = report.relay_mode {
        debug!(relay_mode = %mode, "[{tag}] relay completed");
    }

    let is_motd = session.kind() == ConnectionKind::Motd;
    let level = if is_motd { tracing::Level::DEBUG } else { tracing::Level::INFO };
    tracing::event!(
        level,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        upload_bytes = report.connection_traffic.upload_bytes,
        download_bytes = report.connection_traffic.download_bytes,
        total_connections = ctx.runtime().stats.total_connections(),
        active_remaining = remaining,
        target_addr = report.target_addr.as_ref().map(ToString::to_string).as_deref(),
        "[{tag}] connection closed"
    );
}

#[derive(Debug)]
pub(crate) struct HandledConnection(pub(crate) ConnectionReport);

impl std::fmt::Display for HandledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("connection already handled")
    }
}

impl std::error::Error for HandledConnection {}
