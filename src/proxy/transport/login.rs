use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use crate::minecraft::{decode_login_hello, login_disconnect_packet, FramedPacket, RuntimeAddress};
use crate::proxy::routing::JoinDecision;

use super::super::context::Context as ProxyContext;
use super::super::players::PlayerState;
use super::super::stats::{ConnectionReport, ConnectionRoute, ConnectionSession, ConnectionTraffic};

pub async fn resolve_login_route(
    client: &mut tokio::net::TcpStream,
    ctx: &ProxyContext,
    session: &ConnectionSession,
    login_start_packet: &FramedPacket,
    peer_addr: Option<SocketAddr>,
) -> anyhow::Result<Result<ConnectionRoute, ConnectionReport>> {
    let services = ctx.services();
    let players = &ctx.core.players;
    let _guard = session.enter_stage("CONNECT/LOGIN");
    let login_hello = decode_login_hello(login_start_packet)
        .map_err(anyhow::Error::from)
        .context("decode login hello")?;
    players.update_login(
        session.id,
        login_hello.username.clone(),
        login_hello.profile_id,
    );
    session.record_player_name(&login_hello.username);

    debug!(
        player_name = %login_hello.username,
        player_uuid = ?login_hello.profile_id,
        login_start_bytes = login_start_packet.wire_len,
        "[CONNECT/LOGIN] parsed login hello"
    );

    match services.api.join(
        Some(&login_hello.username),
        login_hello
            .profile_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        peer_addr.as_ref().map(ToString::to_string).as_deref(),
        players.current_online_count(),
    ).await {
        Ok(JoinDecision::Allow(target)) => {
            info!(
                target_addr = %target.target_addr,
                rewrite_addr = ?target.rewrite_addr,
                "[CONNECT/LOGIN] API allowed player join"
            );
            if let Some(cid) = target.connection_id {
                players.update_external_connection_id(session.id, Arc::<str>::from(cid));
            }
            Ok(Ok(ConnectionRoute {
                target_addr: RuntimeAddress::parse(&target.target_addr)
                    .map_err(anyhow::Error::msg)
                    .context("parse login target address")?,
                rewrite_addr: target
                    .rewrite_addr
                    .as_deref()
                    .map(RuntimeAddress::parse)
                    .transpose()
                    .map_err(anyhow::Error::msg)
                    .context("parse login rewrite address")?,
            }))
        }
        Ok(JoinDecision::Deny { kick_reason }) => {
            info!(
                kick_reason = %kick_reason,
                "[CONNECT/LOGIN] API denied player join"
            );
            let kick_packet = login_disconnect_packet(&kick_reason)
                .map_err(anyhow::Error::from)
                .context("build disconnect packet")?;
            client.write_all(&kick_packet).await?;
            client.shutdown().await?;
            players.set_state(session.id, PlayerState::LoginRejectedLocally);

            debug!(
                login_start_bytes = login_start_packet.wire_len,
                kick_packet_bytes = kick_packet.len(),
                "[CONNECT/LOGIN] rejected login with API kick packet"
            );

            Ok(Err(ConnectionReport::new(
                ConnectionTraffic::default(),
                None,
                None,
                None,
            )))
        }
        Err(error) => Err(error),
    }
}
