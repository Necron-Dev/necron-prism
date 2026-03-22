use std::net::SocketAddr;

use super::super::relay::RelayMode;
use super::super::stats::ConnectionTraffic;

#[derive(Clone, Copy, Debug)]
pub struct ConnectionContext {
    pub id: u64,
    pub peer_addr: Option<SocketAddr>,
}

#[derive(Clone, Debug)]
pub struct ConnectionReport {
    pub traffic: ConnectionTraffic,
    pub relay_mode: Option<RelayMode>,
    pub outbound_name: Option<String>,
    pub external_connection_id: Option<String>,
}

impl ConnectionReport {
    pub fn new(
        traffic: ConnectionTraffic,
        relay_mode: Option<RelayMode>,
        outbound_name: Option<String>,
        external_connection_id: Option<String>,
    ) -> Self {
        Self {
            traffic,
            relay_mode,
            outbound_name,
            external_connection_id,
        }
    }
}
