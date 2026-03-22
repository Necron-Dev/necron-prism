use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::client::ApiClient;
use super::types::{JoinDecision, JoinTarget, TrafficEntry, TrafficSnapshot};
use crate::proxy::config::{ApiConfig, ApiMode};

pub struct ApiService {
    runtime: tokio::runtime::Runtime,
    mode: ApiMode,
    client: Option<ApiClient>,
    config: ApiConfig,
    gate: Mutex<()>,
    mock_counter: AtomicU64,
}

impl ApiService {
    pub fn new(config: &ApiConfig) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build api runtime: {error}"))?;
        let client = match config.mode {
            ApiMode::Http => Some(
                ApiClient::new(config)
                    .map_err(|error| format!("failed to build api client: {error}"))?,
            ),
            ApiMode::Mock => None,
        };

        Ok(Self {
            runtime,
            mode: config.mode,
            client,
            config: config.clone(),
            gate: Mutex::new(()),
            mock_counter: AtomicU64::new(0),
        })
    }

    pub fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision, String> {
        match self.mode {
            ApiMode::Http => {
                let _guard = self.gate.lock().expect("api runtime poisoned");
                self.runtime
                    .block_on(
                        self.client
                            .as_ref()
                            .expect("http api client")
                            .join(name, uuid, addr, load),
                    )
                    .map_err(|error| format!("join api request failed: {error}"))
            }
            ApiMode::Mock => Ok(self.mock_join()),
        }
    }

    pub fn traffic_single(
        &self,
        connection_id: &str,
        send_bytes: u64,
        recv_bytes: u64,
    ) -> Result<Vec<String>, String> {
        match self.mode {
            ApiMode::Http => {
                let mut snapshot = TrafficSnapshot::default();
                snapshot.entries.insert(
                    connection_id.to_string(),
                    TrafficEntry {
                        send_bytes,
                        recv_bytes,
                    },
                );

                let _guard = self.gate.lock().expect("api runtime poisoned");
                self.runtime
                    .block_on(
                        self.client
                            .as_ref()
                            .expect("http api client")
                            .traffic(&snapshot),
                    )
                    .map_err(|error| format!("traffic api request failed: {error}"))
            }
            ApiMode::Mock => Ok(Vec::new()),
        }
    }

    pub fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<(), String> {
        match self.mode {
            ApiMode::Http => {
                let _guard = self.gate.lock().expect("api runtime poisoned");
                self.runtime
                    .block_on(
                        self.client
                            .as_ref()
                            .expect("http api client")
                            .closed(cid, send, recv),
                    )
                    .map_err(|error| format!("closed api request failed: {error}"))
            }
            ApiMode::Mock => Ok(()),
        }
    }

    fn mock_join(&self) -> JoinDecision {
        if let Some(kick_reason) = &self.config.mock.kick_reason {
            return JoinDecision::Deny {
                kick_reason: kick_reason.clone(),
            };
        }

        let sequence = self.mock_counter.fetch_add(1, Ordering::Relaxed) + 1;
        JoinDecision::Allow(JoinTarget {
            target_addr: self.config.mock.target_addr.clone(),
            rewrite_addr: self.config.mock.target_addr.clone(),
            connection_id: format!("{}-{sequence}", self.config.mock.connection_id_prefix),
        })
    }
}
