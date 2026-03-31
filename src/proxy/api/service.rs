use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "http-api")]
use super::client::ApiClient;
use crate::proxy::config::{ApiConfig, ApiMode, MockApiConfig};
use crate::proxy::routing::{JoinDecision, JoinTarget};

pub enum ApiService {
    #[cfg(feature = "http-api")]
    Http(HttpApiService),
    Mock(MockApiService),
}

#[cfg(feature = "http-api")]
pub struct HttpApiService {
    runtime: tokio::runtime::Runtime,
    client: ApiClient,
}

pub struct MockApiService {
    target_addr: Arc<str>,
    rewrite_addr: Option<Arc<str>>,
    kick_reason: Option<Arc<str>>,
    connection_id_prefix: Arc<str>,
    counter: AtomicU64,
}

impl ApiService {
    pub fn new(config: &ApiConfig) -> Result<Self> {
        match config.mode {
            #[cfg(feature = "http-api")]
            ApiMode::Http => Ok(Self::Http(HttpApiService::new(config)?)),
            #[cfg(not(feature = "http-api"))]
            ApiMode::Http => Err(anyhow!("http api support is disabled at compile time")),
            ApiMode::Mock => Ok(Self::Mock(MockApiService::new(&config.mock))),
        }
    }

    pub fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision> {
        match self {
            #[cfg(feature = "http-api")]
            Self::Http(service) => service.join(name, uuid, addr, load),
            Self::Mock(service) => service.join(name, uuid, addr, load),
        }
    }

    pub fn traffic_single(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>> {
        match self {
            #[cfg(feature = "http-api")]
            Self::Http(service) => service.traffic_single(connection_id, send_bytes, recv_bytes),
            Self::Mock(service) => service.traffic_single(connection_id, send_bytes, recv_bytes),
        }
    }

    pub fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<()> {
        match self {
            #[cfg(feature = "http-api")]
            Self::Http(service) => service.closed(cid, send, recv),
            Self::Mock(service) => service.closed(cid, send, recv),
        }
    }
}

#[cfg(feature = "http-api")]
impl HttpApiService {
    fn new(config: &ApiConfig) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| anyhow!("failed to build api runtime: {error}"))?;

        Ok(Self {
            runtime,
            client: ApiClient::new(config)?,
        })
    }

    fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision> {
        self.runtime
            .block_on(self.client.join(name, uuid, addr, load))
            .map_err(|error| anyhow!("join api request failed: {error}"))
    }

    fn traffic_single(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>> {
        self.runtime
            .block_on(self.client.traffic(connection_id, send_bytes, recv_bytes))
            .map_err(|error| anyhow!("traffic api request failed: {error}"))
    }

    fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<()> {
        self.runtime
            .block_on(self.client.closed(cid, send, recv))
            .map_err(|error| anyhow!("closed api request failed: {error}"))
    }
}

impl MockApiService {
    fn new(config: &MockApiConfig) -> Self {
        Self {
            target_addr: Arc::<str>::from(config.target_addr.as_str()),
            rewrite_addr: config.rewrite_addr.as_deref().map(Arc::<str>::from),
            connection_id_prefix: Arc::<str>::from(config.connection_id_prefix.as_str()),
            kick_reason: config.kick_reason.as_deref().map(Arc::<str>::from),
            counter: AtomicU64::new(0),
        }
    }

    fn join(
        &self,
        _name: Option<&str>,
        _uuid: Option<&str>,
        _addr: Option<&str>,
        _load: i32,
    ) -> Result<JoinDecision> {
        if let Some(kick_reason) = &self.kick_reason {
            return Ok(JoinDecision::Deny {
                kick_reason: kick_reason.to_string(),
            });
        }

        let sequence = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(JoinDecision::Allow(JoinTarget {
            target_addr: self.target_addr.to_string(),
            rewrite_addr: self.rewrite_addr.as_ref().map(|a| a.to_string()),
            connection_id: Some(format!("{}-{sequence}", self.connection_id_prefix)),
        }))
    }

    fn traffic_single(
        &self,
        _connection_id: &str,
        _send_bytes: u64,
        _recv_bytes: u64,
    ) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn closed(&self, _cid: &str, _send: u64, _recv: u64) -> Result<()> {
        Ok(())
    }
}
