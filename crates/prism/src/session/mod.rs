use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::{field::Empty, info_span, Span};

use necron_prism_minecraft::RuntimeAddress;

use crate::relay::RelayMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionTraffic {
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

impl ConnectionTraffic {
    pub fn combined_with(self, other: Self) -> Self {
        Self {
            upload_bytes: self.upload_bytes + other.upload_bytes,
            download_bytes: self.download_bytes + other.download_bytes,
        }
    }

    pub fn total_bytes(self) -> u64 {
        self.upload_bytes + self.download_bytes
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionSession {
    pub id: u64,
    pub peer_addr: Option<SocketAddr>,
    root_span: Span,
    upload_bytes: Arc<AtomicU64>,
    download_bytes: Arc<AtomicU64>,
}

impl ConnectionSession {
    pub fn new(id: u64, peer_addr: Option<SocketAddr>) -> Self {
        let root_span = info_span!(
            "connection",
            connection_id = id,
            peer_addr = ?peer_addr,
            player_name = Empty,
        );

        Self {
            id,
            peer_addr,
            root_span,
            upload_bytes: Arc::new(AtomicU64::new(0)),
            download_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn root_span(&self) -> &Span {
        &self.root_span
    }

    pub fn record_player_name(&self, player_name: &str) {
        self.root_span.record("player_name", player_name);
    }

    pub fn enter_stage(&self, _stage: &str) -> tracing::span::Entered<'_> {
        self.root_span.enter()
    }

    pub fn add_upload(&self, bytes: u64) {
        self.upload_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_download(&self, bytes: u64) {
        self.download_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn upload(&self) -> u64 {
        self.upload_bytes.load(Ordering::Relaxed)
    }

    pub fn download(&self) -> u64 {
        self.download_bytes.load(Ordering::Relaxed)
    }

    pub fn connection_traffic(&self) -> ConnectionTraffic {
        ConnectionTraffic {
            upload_bytes: self.upload(),
            download_bytes: self.download(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionRoute {
    pub target_addr: RuntimeAddress,
    pub rewrite_addr: Option<RuntimeAddress>,
    pub external_connection_id: Option<Arc<str>>,
}

#[derive(Clone, Debug)]
pub struct ConnectionReport {
    pub connection_traffic: ConnectionTraffic,
    pub relay_mode: Option<RelayMode>,
    #[allow(dead_code)]
    pub target_addr: Option<RuntimeAddress>,
    #[allow(dead_code)]
    pub rewrite_addr: Option<RuntimeAddress>,
}

impl ConnectionReport {
    pub fn new(
        connection_traffic: ConnectionTraffic,
        relay_mode: Option<RelayMode>,
        target_addr: Option<RuntimeAddress>,
        rewrite_addr: Option<RuntimeAddress>,
    ) -> Self {
        Self {
            connection_traffic,
            relay_mode,
            target_addr,
            rewrite_addr,
        }
    }
}

#[cfg(test)]
mod test;
