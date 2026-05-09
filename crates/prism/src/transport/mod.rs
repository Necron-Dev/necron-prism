use std::time::{Duration, Instant};

use anyhow::{Context as AnyhowContext, anyhow};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tracing::{debug, info, trace, warn};

use prism_minecraft::{
    INTENT_STATUS, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE, PRISM_MAGIC_ID, PacketIo,
    decode_handshake, encode_handshake, encode_raw_frame,
};

use crate::context::PrismContext;
use crate::hooks::{LoginResult, PrismHooks};
use crate::outbound::connect_addr as connect_outbound_addr;
use crate::relay::relay_bidirectional;
use crate::session::{
    ConnectionKind, ConnectionReport, ConnectionRoute, ConnectionSession, ConnectionTraffic,
};

fn to_disconnect_json(message: &str) -> String {
    serde_json::json!({ "text": message }).to_string()
}

pub async fn handle_connection<H: PrismHooks>(
    ctx: PrismContext<H>,
    client: tokio::net::TcpStream,
    session: ConnectionSession,
) {
    let started_at = Instant::now();

    let outcome = match handle_client(client, &ctx, &session).await {
        Ok(report) => ConnectionOutcome::Completed(report),
        Err(error) => match error.downcast::<HandledConnection>() {
            Ok(handled) => ConnectionOutcome::Handled(handled.0),
            Err(error) => {
                let report = ConnectionReport::new(session.connection_traffic(), None, None, None);
                let expected_disconnect = is_expected_disconnect(&error);
                ConnectionOutcome::Failed {
                    report,
                    error,
                    expected_disconnect,
                }
            }
        },
    };

    finalize_connection(&ctx, &session, started_at, outcome);
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

    trace!(
        elapsed_ms = started_at.elapsed().as_millis(),
        "[CONNECT] connection handling started"
    );

    let config = ctx.config();
    let first_packet_timeout = Duration::from_millis(config.network.socket.first_packet_timeout_ms);

    let mut packet_io = PacketIo::new(config.network.buffer.packet_read_buffer_size);
    let mut first_byte = [0_u8; 1];
    timeout(first_packet_timeout, client.read_exact(&mut first_byte))
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
        debug!("[CONNECT/MOTD] detected legacy ping (0xFE)");
        let online_count = ctx.runtime().connections.current_online_count();
        let config = ctx.config();
        ctx.hooks()
            .on_legacy_ping(&mut client, session, &config, online_count)
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

    trace!(
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
        let online_count = ctx.runtime().connections.current_online_count();
        let config = ctx.config();
        ctx.hooks()
            .on_status_request(
                &mut packet_io,
                &mut client,
                session,
                &handshake,
                &config,
                online_count,
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

    session.set_kind(ConnectionKind::Proxy);

    let login_start_packet = packet_io
        .read_frame(&mut client, MAX_LOGIN_PACKET_SIZE)
        .await
        .context("read login start packet")?;

    trace!(
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        "[CONNECT/LOGIN] read login start packet"
    );

    let online_count = ctx.runtime().connections.current_online_count();
    let config = ctx.config();
    let login_result = ctx
        .hooks()
        .on_login(
            &mut client,
            session,
            &handshake,
            &login_start_packet,
            session.peer_addr,
            &config,
            online_count,
        )
        .await?;

    let route = match login_result {
        LoginResult::Allow(route) => route,
        LoginResult::Deny { kick_reason } => {
            info!(
                kick_reason = %kick_reason,
                "[CONNECT/LOGIN] player join denied"
            );
            let kick_packet =
                prism_minecraft::login_disconnect_packet(&to_disconnect_json(&kick_reason))
                    .map_err(anyhow::Error::from)
                    .context("build disconnect packet")?;
            client.write_all(&kick_packet).await?;
            client.shutdown().await?;

            trace!(
                login_start_bytes = login_start_packet.wire_len,
                kick_packet_bytes = kick_packet.len(),
                "[CONNECT/LOGIN] rejected login with kick packet"
            );

            return Err(anyhow::Error::new(HandledConnection(
                ConnectionReport::new(ConnectionTraffic::default(), None, None, None),
            )));
        }
    };

    proxy_connection(
        client,
        ctx,
        session,
        handshake,
        login_start_packet,
        route,
        started_at,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn proxy_connection<H: PrismHooks>(
    client: tokio::net::TcpStream,
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
    mut handshake: prism_minecraft::HandshakeInfo,
    login_start_packet: prism_minecraft::FramedPacket,
    route: ConnectionRoute,
    started_at: Instant,
) -> anyhow::Result<ConnectionReport> {
    let _guard = session.enter_stage("CONNECT/OUTBOUND");
    let config = ctx.config();

    let rewrite_addr = route.rewrite_addr.as_ref().unwrap_or(&route.target_addr);
    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|e| anyhow!(e))
        .context("rewrite handshake")?;

    // Login succeeded, set connection_id and register to Registry
    if let Some(cid) = &route.connection_id {
        let session_mut = session.clone();
        session_mut.set_connection_id(cid.to_string());
        let remaining = ctx.runtime().connections.register(session_mut)?;
        ctx.runtime()
            .connections
            .update_outbound(cid, route.target_addr.as_str().into());
        trace!(
            connection_id = %cid,
            active_remaining = remaining,
            "[CONNECT/OUTBOUND] registered connection"
        );
    }

    let rewritten_packet = encode_handshake(&handshake)
        .map_err(anyhow::Error::from)
        .context("encode handshake")?;

    trace!(
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

    if let Some(connection_id) = &route.connection_id {
        ctx.hooks().on_connection_established(
            session,
            connection_id,
            route.player_name.as_deref(),
            route.player_uuid.as_deref(),
        );
    }

    let encoded_login_start = encode_raw_frame(&login_start_packet)
        .map_err(anyhow::Error::from)
        .context("encode login start")?;

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

    Ok(report)
}

fn finalize_connection<H: PrismHooks>(
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
    started_at: Instant,
    outcome: ConnectionOutcome,
) {
    let report = outcome.report().clone();
    ctx.hooks().on_connection_finished(session, &report);
    let _settled = ctx
        .runtime()
        .totals
        .record_finished_connection(report.connection_traffic);

    if let Some(cid) = session.connection_id() {
        let remaining = ctx.runtime().connections.remove_connection(&cid);
        trace!(
            connection_id = %cid,
            active_remaining = remaining,
            "[FINISH] removed connection from registry"
        );
    }

    let tag = session.kind().tag();
    let elapsed_ms = started_at.elapsed().as_millis() as u64;
    let active_remaining = ctx.runtime().connections.active_count();

    if let Some(mode) = report.relay_mode {
        trace!(relay_mode = %mode, "[{tag}] relay completed");
    }

    match outcome {
        ConnectionOutcome::Completed(_) | ConnectionOutcome::Handled(_) => {
            if session.kind() == ConnectionKind::Motd {
                debug!(
                    elapsed_ms,
                    upload_bytes = report.connection_traffic.upload_bytes,
                    download_bytes = report.connection_traffic.download_bytes,
                    active_remaining,
                    "[{tag}] connection closed"
                );
            } else {
                info!(
                    elapsed_ms,
                    upload_bytes = report.connection_traffic.upload_bytes,
                    download_bytes = report.connection_traffic.download_bytes,
                    active_remaining,
                    target_addr = report
                        .target_addr
                        .as_ref()
                        .map(ToString::to_string)
                        .as_deref(),
                    "[{tag}] connection closed"
                );
            }
        }
        ConnectionOutcome::Failed {
            error,
            expected_disconnect,
            ..
        } => {
            if expected_disconnect {
                if session.kind() == ConnectionKind::Motd {
                    debug!(
                        error = %error,
                        elapsed_ms,
                        upload_bytes = report.connection_traffic.upload_bytes,
                        download_bytes = report.connection_traffic.download_bytes,
                        active_remaining,
                        "[{tag}] connection closed"
                    );
                } else {
                    info!(
                        error = %error,
                        elapsed_ms,
                        upload_bytes = report.connection_traffic.upload_bytes,
                        download_bytes = report.connection_traffic.download_bytes,
                        active_remaining,
                        target_addr = report.target_addr.as_ref().map(ToString::to_string).as_deref(),
                        "[{tag}] connection closed"
                    );
                }
            } else {
                warn!(
                    error = %error,
                    elapsed_ms,
                    upload_bytes = report.connection_traffic.upload_bytes,
                    download_bytes = report.connection_traffic.download_bytes,
                    active_remaining,
                    target_addr = report.target_addr.as_ref().map(ToString::to_string).as_deref(),
                    "[{tag}] connection failed"
                );
            }
        }
    }
}

enum ConnectionOutcome {
    Completed(ConnectionReport),
    Handled(ConnectionReport),
    Failed {
        report: ConnectionReport,
        error: anyhow::Error,
        expected_disconnect: bool,
    },
}

impl ConnectionOutcome {
    fn report(&self) -> &ConnectionReport {
        match self {
            Self::Completed(report) | Self::Handled(report) => report,
            Self::Failed { report, .. } => report,
        }
    }
}

#[derive(Debug)]
pub(crate) struct HandledConnection(pub(crate) ConnectionReport);

impl std::fmt::Display for HandledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("connection already handled")
    }
}

impl std::error::Error for HandledConnection {}
