use std::net::SocketAddr;
use std::sync::Arc;

use super::super::relay::RelayMode;
use super::super::stats::ConnectionTraffic;

#[derive(Clone, Copy, Debug)]
pub struct ConnectionContext {
    pub id: u64,
    pub peer_addr: Option<SocketAddr>,
}

#[derive(Clone, Debug)]
pub struct ConnectionRoute {
    pub target_addr: Arc<str>,
    pub rewrite_addr: Arc<str>,
}

#[derive(Clone, Debug)]
pub struct ConnectionReport {
    pub traffic: ConnectionTraffic,
    pub relay_mode: Option<RelayMode>,
    pub target_addr: Arc<str>,
    pub rewrite_addr: Arc<str>,
}

impl ConnectionReport {
    pub fn new(
        traffic: ConnectionTraffic,
        relay_mode: Option<RelayMode>,
        target_addr: Arc<str>,
        rewrite_addr: Arc<str>,
    ) -> Self {
        Self {
            traffic,
            relay_mode,
            target_addr,
            rewrite_addr,
        }
    }
}
