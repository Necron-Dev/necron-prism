use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::{info, trace};

use necron_prism_minecraft::{
    decode_login_hello, FramedPacket, HandshakeInfo, PacketIo, RuntimeAddress,
};
use prism::{
    Config, ConnectionReport, ConnectionRoute, ConnectionSession, LoginResult, PrismHooks,
};

use super::api::ApiService;
use super::routing::JoinDecision;
use super::traffic::TrafficReporter;

fn offline_uuid(username: &str) -> uuid::Uuid {
    uuid::Uuid::new_v3(&uuid::Uuid::NAMESPACE_DNS, format!("OfflinePlayer:{username}").as_bytes())
}

pub struct NecronPrismHooks {
    api: Arc<ApiService>,
    motd: Arc<prism::motd::MotdService>,
    traffic: TrafficReporter,
}

impl NecronPrismHooks {
    pub fn new(
        api: Arc<ApiService>,
        motd: Arc<prism::motd::MotdService>,
        traffic: TrafficReporter,
    ) -> Self {
        Self { api, motd, traffic }
    }
}

impl PrismHooks for NecronPrismHooks {
    async fn on_legacy_ping(
        &self,
        client: &mut tokio::net::TcpStream,
        _session: &ConnectionSession,
        config: &Config,
        online_count: i32,
    ) -> Result<()> {
        // MOTD 请求不需要 connection_id
        prism::motd::serve_legacy_ping(
            client,
            &config.motd,
            &config.network.relay,
            &self.motd,
            0, // legacy ping 不需要 connection_id
            online_count,
        )
        .await
    }

    async fn on_status_request(
        &self,
        packet_io: &mut PacketIo,
        client: &mut tokio::net::TcpStream,
        session: &ConnectionSession,
        handshake: &HandshakeInfo,
        config: &Config,
        online_count: i32,
    ) -> Result<()> {
        self.motd
            .serve(
                packet_io,
                client,
                &config.motd,
                &config.network.relay,
                online_count,
                handshake,
                session,
            )
            .await
    }

    async fn on_login(
        &self,
        _client: &mut tokio::net::TcpStream,
        session: &ConnectionSession,
        handshake: &HandshakeInfo,
        login_start_packet: &FramedPacket,
        peer_addr: Option<SocketAddr>,
        config: &Config,
        online_count: i32,
    ) -> Result<LoginResult> {
        let _guard = session.enter_stage("CONNECT/LOGIN");
        let login_hello = decode_login_hello(login_start_packet)
            .map_err(anyhow::Error::from)
            .context("decode login hello")?;

        let player_uuid = login_hello
            .profile_id
            .unwrap_or_else(|| offline_uuid(&login_hello.username));

        session.record_player_identity(&login_hello.username, &player_uuid.to_string());

        trace!(
            player_name = %login_hello.username,
            player_uuid = %player_uuid,
            login_start_bytes = login_start_packet.wire_len,
            "[CONNECT/LOGIN] parsed login hello"
        );

        match self
            .api
            .join(
                Some(&login_hello.username),
                Some(&player_uuid.to_string()),
                peer_addr.as_ref().map(ToString::to_string).as_deref(),
                Some(&handshake.server_address),
                &config.api.entry_node_key.clone().unwrap_or("default".to_string()),
                online_count,
            )
            .await
        {
            Ok(JoinDecision::Allow(target)) => {
                info!(
                    target_addr = %target.target_addr,
                    rewrite_addr = ?target.rewrite_addr,
                    "[CONNECT/LOGIN] API allowed player join"
                );

                let target_addr = RuntimeAddress::parse(&target.target_addr)
                    .map_err(anyhow::Error::msg)
                    .context("parse login target address")?;
                let rewrite_addr = target
                    .rewrite_addr
                    .as_deref()
                    .map(RuntimeAddress::parse)
                    .transpose()
                    .map_err(anyhow::Error::msg)
                    .context("parse login rewrite address")?;
                let connection_id = target.connection_id.map(Arc::<str>::from);
                Ok(LoginResult::Allow(ConnectionRoute {
                    target_addr,
                    rewrite_addr,
                    connection_id,
                    player_name: Some(Arc::<str>::from(login_hello.username.clone())),
                    player_uuid: Some(Arc::<str>::from(player_uuid.to_string())),
                }))
            }
            Ok(JoinDecision::Deny { kick_reason }) => {
                info!(
                    kick_reason = %kick_reason,
                    "[CONNECT/LOGIN] API denied player join"
                );
                Ok(LoginResult::Deny { kick_reason })
            }
            Err(error) => Err(error),
        }
    }

    fn on_connection_established(
        &self,
        session: &ConnectionSession,
        connection_id: &str,
        player_name: Option<&str>,
        player_uuid: Option<&str>,
    ) {
        self.traffic.register(
            connection_id,
            session.clone(),
            player_name.map(|n| Arc::<str>::from(n.to_owned())),
            player_uuid.map(|u| Arc::<str>::from(u.to_owned())),
            None,
        );
    }

    fn on_connection_finished(
        &self,
        session: &ConnectionSession,
        report: &ConnectionReport,
    ) {
        if let Some(cid) = session.connection_id() {
            self.traffic.finish(&cid, report.connection_traffic);
        }
    }
}
