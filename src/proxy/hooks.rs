use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::{debug, info};

use necron_prism_minecraft::{
    decode_login_hello, FramedPacket, HandshakeInfo, PacketIo, RuntimeAddress,
};
use prism::{
    Config, ConnectionReport, ConnectionRoute, ConnectionSession, LoginResult, PrismHooks,
};

use super::api::ApiService;
use super::routing::JoinDecision;
use super::traffic::TrafficReporter;

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
        session: &ConnectionSession,
        config: &Config,
        online_count: i32,
    ) -> Result<()> {
        prism::motd::serve_legacy_ping(
            client,
            &config.motd,
            &config.network.relay,
            &self.motd,
            session.id,
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
        login_start_packet: &FramedPacket,
        peer_addr: Option<SocketAddr>,
        _config: &Config,
        online_count: i32,
    ) -> Result<LoginResult> {
        let _guard = session.enter_stage("CONNECT/LOGIN");
        let login_hello = decode_login_hello(login_start_packet)
            .map_err(anyhow::Error::from)
            .context("decode login hello")?;

        session.record_player_name(&login_hello.username);

        debug!(
            player_name = %login_hello.username,
            player_uuid = ?login_hello.profile_id,
            login_start_bytes = login_start_packet.wire_len,
            "[CONNECT/LOGIN] parsed login hello"
        );

        match self
            .api
            .join(
                Some(&login_hello.username),
                login_hello
                    .profile_id
                    .as_ref()
                    .map(ToString::to_string)
                    .as_deref(),
                peer_addr.as_ref().map(ToString::to_string).as_deref(),
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
                let external_connection_id = target.connection_id.map(|id| Arc::<str>::from(id));

                Ok(LoginResult::Allow(ConnectionRoute {
                    target_addr,
                    rewrite_addr,
                    external_connection_id,
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
        external_connection_id: &str,
    ) {
        self.traffic
            .register(session.id, external_connection_id, session.clone(), None);
    }

    fn on_connection_finished(
        &self,
        session: &ConnectionSession,
        report: &ConnectionReport,
    ) {
        self.traffic.finish(session.id, report.connection_traffic);
    }
}
