use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum JoinDecision {
    Allow {
        server_ip: String,
        connection_id: String,
    },
    Deny {
        kick_reason: String,
    },
}

#[derive(Clone, Debug, Default)]
pub struct TrafficSnapshot {
    pub entries: BTreeMap<String, TrafficEntry>,
}

#[derive(Clone, Debug)]
pub struct TrafficEntry {
    pub send_bytes: u64,
    pub recv_bytes: u64,
}
