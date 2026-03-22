use std::sync::Mutex;

use super::client::ApiClient;
use super::types::{JoinDecision, TrafficEntry, TrafficSnapshot};
use crate::proxy::config::ApiConfig;

pub struct ApiService {
    runtime: tokio::runtime::Runtime,
    client: ApiClient,
    gate: Mutex<()>,
}

impl ApiService {
    pub fn new(config: &ApiConfig) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build api runtime: {error}"))?;
        let client = ApiClient::new(config)
            .map_err(|error| format!("failed to build api client: {error}"))?;

        Ok(Self {
            runtime,
            client,
            gate: Mutex::new(()),
        })
    }

    pub fn join(
        &self,
        name: Option<&str>,
        uuid: Option<&str>,
        addr: Option<&str>,
        load: i32,
    ) -> Result<JoinDecision, String> {
        let _guard = self.gate.lock().expect("api runtime poisoned");
        self.runtime
            .block_on(self.client.join(name, uuid, addr, load))
            .map_err(|error| format!("join api request failed: {error}"))
    }

    pub fn traffic_single(
        &self,
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

        let _guard = self.gate.lock().expect("api runtime poisoned");
        self.runtime
            .block_on(self.client.traffic(&snapshot))
            .map_err(|error| format!("traffic api request failed: {error}"))
    }

    pub fn closed(&self, cid: &str, send: u64, recv: u64) -> Result<(), String> {
        let _guard = self.gate.lock().expect("api runtime poisoned");
        self.runtime
            .block_on(self.client.closed(cid, send, recv))
            .map_err(|error| format!("closed api request failed: {error}"))
    }
}

impl Clone for ApiService {
    fn clone(&self) -> Self {
        panic!("ApiService should be wrapped in Arc and not cloned directly")
    }
}
