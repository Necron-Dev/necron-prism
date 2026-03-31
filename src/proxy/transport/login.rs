use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::minecraft::{decode_login_hello, login_disconnect_packet, FramedPacket};
use crate::proxy::routing::JoinDecision;

use super::super::api::ApiService;
use super::super::players::{PlayerRegistry, PlayerState};
use super::super::stats::ConnectionTraffic;
use super::types::{ConnectionReport, ConnectionRoute};

pub async fn resolve_login_route(
    client: &mut tokio::net::TcpStream,
    api: &ApiService,
    players: &PlayerRegistry,
    connection_id: u64,
    login_start_packet: &FramedPacket,
    peer_addr: Option<SocketAddr>,
) -> anyhow::Result<Result<ConnectionRoute, ConnectionReport>> {
    let login_hello = decode_login_hello(login_start_packet)
        .map_err(anyhow::Error::from)
        .context("decode login hello")?;
    players.update_login(
        connection_id,
        login_hello.username.clone(),
        login_hello.profile_id,
    );

    info!(
        player_name = %login_hello.username,
        player_uuid = ?login_hello.profile_id,
        login_start_bytes = login_start_packet.wire_len,
        "parsed login hello"
    );

    match api.join(
        Some(&login_hello.username),
        login_hello
            .profile_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        peer_addr.as_ref().map(ToString::to_string).as_deref(),
        players.current_online_count(),
    ) {
        Ok(JoinDecision::Allow(target)) => {
            if let Some(cid) = target.connection_id {
                players.update_external_connection_id(connection_id, Arc::<str>::from(cid));
            }
            Ok(Ok(ConnectionRoute {
                target_addr: Arc::<str>::from(target.target_addr),
                rewrite_addr: target.rewrite_addr.map(Arc::<str>::from),
            }))
        }
        Ok(JoinDecision::Deny { kick_reason }) => deny_with_reason(
            client,
            &kick_reason,
            players,
            connection_id,
            login_start_packet,
        )
        .await
        .map(Err),
        Err(error) => Err(error.into()),
    }
}

async fn deny_with_reason(
    client: &mut tokio::net::TcpStream,
    reason: &str,
    players: &PlayerRegistry,
    connection_id: u64,
    login_start_packet: &FramedPacket,
) -> anyhow::Result<ConnectionReport> {
    let kick_packet = login_disconnect_packet(reason)
        .map_err(anyhow::Error::from)
        .context("build disconnect packet")?;
    client.write_all(&kick_packet).await?;
    client.shutdown().await?;
    players.set_state(connection_id, PlayerState::LoginRejectedLocally);

    info!(
        login_start_bytes = login_start_packet.wire_len,
        kick_packet_bytes = kick_packet.len(),
        "rejected login with api kick packet"
    );

    Ok(ConnectionReport::new(
        ConnectionTraffic::default(),
        None,
        None,
        None,
    ))
}
