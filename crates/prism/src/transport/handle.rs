use std::time::{Duration, Instant};

use anyhow::Context as AnyhowContext;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tracing::{debug, info, trace};

use prism_minecraft::{
    HandshakeC2s, HandshakeNextState, MAGIC, MAX_HANDSHAKE_PACKET_SIZE, MAX_LOGIN_PACKET_SIZE,
    PacketIo, decode_handshake,
};

use crate::context::PrismContext;
use crate::hooks::{LoginResult, PrismHooks};
use crate::session::{ConnectionKind, ConnectionReport, ConnectionSession, ConnectionTraffic};

use super::{outcome, proxy};

pub(super) async fn handle_client<H: PrismHooks>(
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

    // https://minecraft.wiki/w/Java_Edition_protocol/Packets#Legacy_Server_List_Ping
    if first_byte[0] == 0xFE {
        return handle_legacy_ping(&mut client, ctx, session).await;
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

    if handshake_packet.frame.id == MAGIC {
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
        protocol_version = handshake.protocol_version.0,
        next_state = ?handshake.next_state,
        original_host = handshake.server_address,
        original_port = handshake.server_port,
        handshake_wire_bytes = handshake_packet.wire_len,
        elapsed_ms = started_at.elapsed().as_millis(),
        "[CONNECT] handshake packet decoded"
    );

    if handshake.next_state == HandshakeNextState::Status {
        return handle_motd(&mut packet_io, &mut client, ctx, session, &handshake).await;
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
            info!(kick_reason = %kick_reason, "[CONNECT/LOGIN] player join denied");
            let kick_packet = prism_minecraft::login_disconnect_packet(
                &serde_json::json!({ "text": kick_reason }).to_string(),
            )
            .map_err(anyhow::Error::from)
            .context("build disconnect packet")?;
            client.write_all(&kick_packet).await?;
            client.shutdown().await?;

            trace!(
                login_start_bytes = login_start_packet.wire_len,
                kick_packet_bytes = kick_packet.len(),
                "[CONNECT/LOGIN] rejected login with kick packet"
            );

            return Err(anyhow::Error::new(outcome::HandledConnection(
                ConnectionReport::new(ConnectionTraffic::default(), None, None, None),
            )));
        }
    };

    proxy::proxy_connection(
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

async fn handle_legacy_ping<H: PrismHooks>(
    client: &mut tokio::net::TcpStream,
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
) -> anyhow::Result<ConnectionReport> {
    session.set_kind(ConnectionKind::Motd);
    let _motd_guard = session.root_span().enter();
    debug!("[CONNECT/MOTD] detected legacy ping (0xFE)");
    let online_count = ctx.runtime().connections.current_online_count();
    let config = ctx.config();
    ctx.hooks()
        .on_legacy_ping(client, session, &config, online_count)
        .await
        .context("serve legacy ping")?;
    Ok(ConnectionReport::new(
        ConnectionTraffic::default(),
        None,
        None,
        None,
    ))
}

async fn handle_motd<H: PrismHooks>(
    packet_io: &mut PacketIo,
    client: &mut tokio::net::TcpStream,
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
    handshake: &HandshakeC2s,
) -> anyhow::Result<ConnectionReport> {
    session.set_kind(ConnectionKind::Motd);
    let _motd_guard = session.root_span().enter();
    let online_count = ctx.runtime().connections.current_online_count();
    let config = ctx.config();
    ctx.hooks()
        .on_status_request(packet_io, client, session, handshake, &config, online_count)
        .await
        .context("serve motd")?;
    Ok(ConnectionReport::new(
        ConnectionTraffic::default(),
        None,
        None,
        None,
    ))
}
