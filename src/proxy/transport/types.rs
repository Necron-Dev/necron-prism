use std::net::SocketAddr;

use super::super::relay::RelayMode;
use super::super::stats::ConnectionTraffic;

#[derive(Clone, Copy, Debug)]
pub struct ConnectionContext {
    pub id: u64,
    pub peer_addr: Option<SocketAddr>,
}

#[derive(Clone, Debug)]
pub struct ConnectionRoute {
    pub target_addr: String,
    pub rewrite_addr: String,
}

#[derive(Clone, Debug)]
pub struct ConnectionReport {
    pub traffic: ConnectionTraffic,
    pub relay_mode: Option<RelayMode>,
    pub target_addr: String,
    pub rewrite_addr: String,
}

impl ConnectionReport {
    pub fn new(
        traffic: ConnectionTraffic,
        relay_mode: Option<RelayMode>,
        target_addr: String,
        rewrite_addr: String,
    ) -> Self {
        Self {
            traffic,
            relay_mode,
            target_addr,
            rewrite_addr,
        }
    }
}
