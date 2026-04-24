use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};
use std::sync::Mutex;

use tracing::{field::Empty, info_span, Span};
use valence_protocol::uuid::Uuid;

use necron_prism_minecraft::RuntimeAddress;

use crate::relay::RelayMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerState {
    Connected,
    Routing,
    Login,
    StatusServedLocally,
    LoginRejectedLocally,
    Proxying,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionKind {
    Unknown = 0,
    Motd = 1,
    Proxy = 2,
}

impl ConnectionKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Unknown => "CONNECT",
            Self::Motd => "CONNECT/MOTD",
            Self::Proxy => "CONNECT/LIFECYCLE",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionSession {
    connection_id: Arc<OnceLock<String>>,
    pub peer_addr: Option<SocketAddr>,

    // Player/connection state fields (merged from PlayerSession)
    pub username: Option<String>,
    pub uuid: Option<Uuid>,
    pub outbound_name: Option<Arc<str>>,
    pub protocol_version: Option<i32>,
    pub next_state: Option<i32>,
    pub state: PlayerState,

    // Connection tracking fields
    kind: Arc<AtomicU8>,
    root_span: Span,
    upload_bytes: Arc<AtomicU64>,
    download_bytes: Arc<AtomicU64>,
}

impl ConnectionSession {
    pub fn new(peer_addr: Option<SocketAddr>) -> Self {
        let root_span = info_span!(
            "connection",
            connection_id = Empty,
            peer_addr = ?peer_addr,
            player_name = Empty,
            player_uuid = Empty,
        );

        Self {
            connection_id: Arc::new(OnceLock::new()),
            peer_addr,
            username: None,
            uuid: None,
            outbound_name: None,
            protocol_version: None,
            next_state: None,
            state: PlayerState::Connected,
            kind: Arc::new(AtomicU8::new(ConnectionKind::Unknown as u8)),
            root_span,
            upload_bytes: Arc::new(AtomicU64::new(0)),
            download_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn set_connection_id(&self, id: String) {
        if self.connection_id.set(id.clone()).is_ok() {
            self.root_span.record("connection_id", id.as_str());
        }
    }

    pub fn connection_id(&self) -> Option<String> {
        self.connection_id.get().cloned()
    }

    pub fn root_span(&self) -> &Span {
        &self.root_span
    }

    pub fn record_player_identity(&self, player_name: &str, player_uuid: &str) {
        self.root_span.record("player_name", player_name);
        self.root_span.record("player_uuid", player_uuid);
    }

    pub fn set_kind(&self, kind: ConnectionKind) {
        self.kind.store(kind as u8, Ordering::Relaxed);
    }

    pub fn kind(&self) -> ConnectionKind {
        match self.kind.load(Ordering::Relaxed) {
            1 => ConnectionKind::Motd,
            2 => ConnectionKind::Proxy,
            _ => ConnectionKind::Unknown,
        }
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
    pub connection_id: Option<Arc<str>>,
    pub player_name: Option<Arc<str>>,
    pub player_uuid: Option<Arc<str>>,
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
