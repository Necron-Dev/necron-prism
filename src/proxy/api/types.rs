use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct JoinTarget {
    pub target_addr: String,
    pub rewrite_addr: String,
    pub connection_id: String,
}

#[derive(Clone, Debug)]
pub enum JoinDecision {
    Allow(JoinTarget),
    Deny { kick_reason: String },
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
