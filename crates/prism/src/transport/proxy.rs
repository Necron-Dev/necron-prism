use std::time::Instant;

use anyhow::{Context as AnyhowContext, anyhow};
use smallvec::SmallVec;
use tokio::io::AsyncWriteExt;
use tracing::{info, trace};

use prism_minecraft::{FramedPacket, HandshakeC2s, encode_handshake, encode_raw_frame};

use crate::context::PrismContext;
use crate::hooks::PrismHooks;
use crate::relay::relay_bidirectional;
use crate::session::{ConnectionReport, ConnectionRoute, ConnectionSession};

/// Inline capacity covers handshake (~32 bytes) + login start (~16 bytes) with headroom.
const COMBINED_BUFFER_INLINE: usize = 512;

pub(super) async fn proxy_connection<H: PrismHooks>(
    client: tokio::net::TcpStream,
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
    mut handshake: HandshakeC2s,
    login_start_packet: FramedPacket,
    route: ConnectionRoute,
    started_at: Instant,
) -> anyhow::Result<ConnectionReport> {
    let _guard = session.root_span().enter();
    let config = ctx.config();

    let rewrite_addr = route.rewrite_addr.as_ref().unwrap_or(&route.target_addr);
    handshake
        .rewrite_addr(rewrite_addr)
        .map_err(|e| anyhow!(e))
        .context("rewrite handshake")?;

    if let Some(cid) = &route.connection_id {
        let session_mut = session.clone();
        session_mut.set_connection_id(cid.to_string());
        let remaining = ctx.runtime().connections.register(session_mut)?;
        ctx.runtime()
            .connections
            .update_outbound(cid, route.target_addr.as_str().into());
        trace!(connection_id = %cid, active_remaining = remaining, "[CONNECT/OUTBOUND] registered connection");
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

    let mut upstream = crate::outbound::connect_addr(&route.target_addr, &config, session)
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

    let mut combined = SmallVec::<[u8; COMBINED_BUFFER_INLINE]>::with_capacity(
        rewritten_packet.len() + encoded_login_start.len(),
    );
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
