use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};

use super::client::ApiClient;
use super::types::{JoinDecision, JoinTarget, TrafficEntry, TrafficSnapshot};
use crate::proxy::config::{ApiConfig, ApiMode, MockApiConfig};

pub struct ApiService {
    runtime: tokio::runtime::Runtime,
    inner: ApiBackend,
}

enum ApiBackend {
    Http(ApiClient),
    Mock(MockApiService),
}

struct MockApiService {
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

        let inner = match config.mode {
            ApiMode::Http => ApiBackend::Http(
                ApiClient::new(config)
                    .map_err(|error| format!("failed to build api client: {error}"))?,
            ),
            ApiMode::Mock => ApiBackend::Mock(MockApiService::new(&config.mock)),
        };

        Ok(Self { runtime, inner })
    }

    pub fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision, String> {
        match &self.inner {
            ApiBackend::Http(client) => self
                .runtime
                .block_on(client.join(name, uuid, addr, load))
                .map_err(|error| format!("join api request failed: {error}")),
            ApiBackend::Mock(mock) => Ok(mock.join()),
        }
    }

    pub fn traffic_single(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>, String> {
        match &self.inner {
            ApiBackend::Http(client) => {
                let mut snapshot = TrafficSnapshot::default();
                snapshot.entries.insert(
                    connection_id.to_string(),
                    TrafficEntry {
                        send_bytes,
                        recv_bytes,
                    },
                );

                self.runtime
                    .block_on(client.traffic(&snapshot))
                    .map_err(|error| format!("traffic api request failed: {error}"))
            }
            ApiBackend::Mock(_) => Ok(Vec::new()),
        }
    }

    pub fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<(), String> {
        match &self.inner {
            ApiBackend::Http(client) => self
                .runtime
                .block_on(client.closed(cid, send, recv))
                .map_err(|error| format!("closed api request failed: {error}")),
            ApiBackend::Mock(_) => Ok(()),
        }
    }
}

impl MockApiService {
    fn new(config: &MockApiConfig) -> Self {
        Self {
            target_addr: config.target_addr.clone(),
            rewrite_addr: config.target_addr.clone(),
            kick_reason: config.kick_reason.clone(),
            connection_id_prefix: config.connection_id_prefix.clone(),
            counter: AtomicU64::new(0),
        }
    }

    fn join(&self) -> JoinDecision {
        if let Some(kick_reason) = &self.kick_reason {
            return JoinDecision::Deny {
                kick_reason: kick_reason.clone(),
            };
        }

        let sequence = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        JoinDecision::Allow(JoinTarget {
            target_addr: self.target_addr.clone(),
            rewrite_addr: self.rewrite_addr.clone(),
            connection_id: format!("{}-{sequence}", self.connection_id_prefix),
        })
    }

    #[allow(dead_code)]
    fn kick_reason(&self) -> Option<Cow<'_, str>> {
        self.kick_reason.as_deref().map(Cow::Borrowed)
    }
}
