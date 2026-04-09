use std::future::Future;
use std::net::SocketAddr;

use anyhow::Result;
use necron_prism_minecraft::{FramedPacket, HandshakeInfo, PacketIo};

use crate::config::Config;
use crate::session::{ConnectionReport, ConnectionRoute, ConnectionSession};

pub trait PrismHooks: Send + Sync + 'static {
    fn on_legacy_ping(
        &self,
        client: &mut tokio::net::TcpStream,
        session: &ConnectionSession,
        config: &Config,
        online_count: i32,
    ) -> impl Future<Output = Result<()>> + Send;

    fn on_status_request(
        &self,
        packet_io: &mut PacketIo,
        client: &mut tokio::net::TcpStream,
        session: &ConnectionSession,
        handshake: &HandshakeInfo,
        config: &Config,
        online_count: i32,
    ) -> impl Future<Output = Result<()>> + Send;

    fn on_login(
        &self,
        client: &mut tokio::net::TcpStream,
        session: &ConnectionSession,
        login_packet: &FramedPacket,
        peer_addr: Option<SocketAddr>,
        config: &Config,
        online_count: i32,
    ) -> impl Future<Output = Result<LoginResult>> + Send;

    fn on_connection_established(
        &self,
        session: &ConnectionSession,
        external_connection_id: &str,
    );

    fn on_connection_finished(
        &self,
        session: &ConnectionSession,
        report: &ConnectionReport,
    );
}

#[derive(Clone, Debug)]
pub enum LoginResult {
    Allow(ConnectionRoute),
    Deny { kick_reason: String },
}
