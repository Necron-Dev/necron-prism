use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "http-api")]
use super::client::ApiClient;
use prism::config::{ApiConfig, ApiMode};
use crate::proxy::routing::{JoinDecision, JoinTarget};

pub enum ApiService {
    #[cfg(feature = "http-api")]
    Http(Box<HttpApiService>),
    Mock(MockApiService),
}

#[cfg(feature = "http-api")]
pub struct HttpApiService {
    client: ApiClient,
}

pub struct MockApiService {
    counter: Arc<AtomicU64>,
    config: ApiConfig,
}

impl ApiService {
    pub fn new(config: &ApiConfig, mock_counter: Arc<AtomicU64>) -> Result<Self> {
        match config.mode {
            #[cfg(feature = "http-api")]
            ApiMode::Http => Ok(Self::Http(Box::new(HttpApiService::new(config)?))),
            #[cfg(not(feature = "http-api"))]
            ApiMode::Http => Err(anyhow!("http api support is disabled at compile time")),
            ApiMode::Mock => Ok(Self::Mock(MockApiService::new(mock_counter, config.clone()))),
        }
    }

    pub async fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision> {
        match self {
            #[cfg(feature = "http-api")]
            Self::Http(service) => service.join(name, uuid, addr, load).await,
            Self::Mock(service) => service.join(name, uuid, addr, load).await,
        }
    }

    pub async fn traffic_single(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>> {
        match self {
            #[cfg(feature = "http-api")]
            Self::Http(service) => service.traffic_single(connection_id, send_bytes, recv_bytes).await,
            Self::Mock(service) => service.traffic_single(connection_id, send_bytes, recv_bytes).await,
        }
    }

    pub async fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<()> {
        match self {
            #[cfg(feature = "http-api")]
            Self::Http(service) => service.closed(cid, send, recv).await,
            Self::Mock(service) => service.closed(cid, send, recv).await,
        }
    }
}

#[cfg(feature = "http-api")]
impl HttpApiService {
    fn new(config: &ApiConfig) -> Result<Self> {
        Ok(Self {
            client: ApiClient::new(config)?,
        })
    }

    async fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision> {
        self.client.join(name, uuid, addr, load).await
            .map_err(|error| anyhow!("join api request failed: {error}"))
    }

    async fn traffic_single(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>> {
        self.client.traffic(connection_id, send_bytes, recv_bytes).await
            .map_err(|error| anyhow!("traffic api request failed: {error}"))
    }

    async fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<()> {
        self.client.closed(cid, send, recv).await
            .map_err(|error| anyhow!("closed api request failed: {error}"))
    }
}

impl MockApiService {
    fn new(counter: Arc<AtomicU64>, config: ApiConfig) -> Self {
        Self { counter, config }
    }

    async fn join(
        &self,
        _name: Option<&str>,
        _uuid: Option<&str>,
        _addr: Option<&str>,
        _load: i32,
    ) -> Result<JoinDecision> {
        if let Some(kick_reason) = &self.config.mock_kick_reason {
            return Ok(JoinDecision::Deny {
                kick_reason: kick_reason.clone(),
            });
        }

        let sequence = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(JoinDecision::Allow(JoinTarget {
            target_addr: self.config.mock_target_addr.clone(),
            rewrite_addr: self.config.mock_rewrite_addr.clone(),
            connection_id: Some(format!("{}-{sequence}", self.config.mock_connection_id_prefix)),
        }))
    }

    async fn traffic_single(
        &self,
        _connection_id: &str,
        _send_bytes: u64,
        _recv_bytes: u64,
    ) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn closed(&self, _cid: &str, _send: u64, _recv: u64) -> Result<()> {
        Ok(())
    }
}
