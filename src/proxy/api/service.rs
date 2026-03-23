use std::sync::atomic::{AtomicU64, Ordering};

use super::client::ApiClient;
use super::types::{JoinDecision, JoinTarget, TrafficEntry, TrafficSnapshot};
use crate::proxy::config::{ApiConfig, ApiMode, MockApiConfig};

pub struct ApiService {
    runtime: tokio::runtime::Runtime,
    backend: Box<dyn ApiBackend>,
}

trait ApiBackend: Send + Sync {
    fn join(
        &self,
        runtime: &tokio::runtime::Runtime,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision, String>;

    fn traffic_single(
        &self,
        runtime: &tokio::runtime::Runtime,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>, String>;

    fn closed(
        &self,
        runtime: &tokio::runtime::Runtime,
        cid: &str,
        send: u64,
        recv: u64,
    ) -> Result<(), String>;
}

struct HttpApiBackend {
    client: ApiClient,
}

struct MockApiBackend {
    target_addr: String,
    rewrite_addr: String,
    kick_reason: Option<String>,
    connection_id_prefix: String,
    counter: AtomicU64,
}

impl ApiService {
    pub fn new(config: &ApiConfig) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build api runtime: {error}"))?;

        let backend: Box<dyn ApiBackend> = match config.mode {
            ApiMode::Http => Box::new(HttpApiBackend::new(config)?),
            ApiMode::Mock => Box::new(MockApiBackend::new(&config.mock)),
        };

        Ok(Self { runtime, backend })
    }

    pub fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision, String> {
        self.backend.join(&self.runtime, name, uuid, addr, load)
    }

    pub fn traffic_single(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>, String> {
        self.backend
            .traffic_single(&self.runtime, connection_id, send_bytes, recv_bytes)
    }

    pub fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<(), String> {
        self.backend.closed(&self.runtime, cid, send, recv)
    }
}

impl HttpApiBackend {
    fn new(config: &ApiConfig) -> Result<Self, String> {
        Ok(Self {
            client: ApiClient::new(config)?,
        })
    }
}

impl ApiBackend for HttpApiBackend {
    fn join(
        &self,
        runtime: &tokio::runtime::Runtime,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision, String> {
        runtime
            .block_on(self.client.join(name, uuid, addr, load))
            .map_err(|error| format!("join api request failed: {error}"))
    }

    fn traffic_single(
        &self,
        runtime: &tokio::runtime::Runtime,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>, String> {
        let mut snapshot = TrafficSnapshot::default();
        snapshot.entries.insert(
            connection_id.to_string(),
            TrafficEntry {
                send_bytes,
                recv_bytes,
            },
        );

        runtime
            .block_on(self.client.traffic(&snapshot))
            .map_err(|error| format!("traffic api request failed: {error}"))
    }

    fn closed(
        &self,
        runtime: &tokio::runtime::Runtime,
        cid: &str,
        send: u64,
        recv: u64,
    ) -> Result<(), String> {
        runtime
            .block_on(self.client.closed(cid, send, recv))
            .map_err(|error| format!("closed api request failed: {error}"))
    }
}

impl MockApiBackend {
    fn new(config: &MockApiConfig) -> Self {
        Self {
            target_addr: config.target_addr.clone(),
            rewrite_addr: config.target_addr.clone(),
            kick_reason: config.kick_reason.clone(),
            connection_id_prefix: config.connection_id_prefix.clone(),
            counter: AtomicU64::new(0),
        }
    }
}

impl ApiBackend for MockApiBackend {
    fn join(
        &self,
        _runtime: &tokio::runtime::Runtime,
        _name: Option<&str>,
        _uuid: Option<&str>,
        _addr: Option<&str>,
        _load: i32,
    ) -> Result<JoinDecision, String> {
        if let Some(kick_reason) = &self.kick_reason {
            return Ok(JoinDecision::Deny {
                kick_reason: kick_reason.clone(),
            });
        }

        let sequence = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(JoinDecision::Allow(JoinTarget {
            target_addr: self.target_addr.clone(),
            rewrite_addr: self.rewrite_addr.clone(),
            connection_id: format!("{}-{sequence}", self.connection_id_prefix),
        }))
    }

    fn traffic_single(
        &self,
        _runtime: &tokio::runtime::Runtime,
        _connection_id: &str,
        _send_bytes: u64,
        _recv_bytes: u64,
    ) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    fn closed(
        &self,
        _runtime: &tokio::runtime::Runtime,
        _cid: &str,
        _send: u64,
        _recv: u64,
    ) -> Result<(), String> {
        Ok(())
    }
}
