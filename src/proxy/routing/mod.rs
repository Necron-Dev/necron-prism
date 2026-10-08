#[derive(Clone, Debug)]
pub struct JoinTarget {
    pub target_addr: String,
    pub rewrite_addr: Option<String>,
    pub connection_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct JoinRequest {
    pub name: Option<String>,
    pub uuid: Option<String>,
    pub peer_addr: Option<String>,
    pub connect_host: Option<String>,
    pub entry_node_key: String,
    pub load: i32,
    pub protocol_version: i32,
}

#[derive(Clone, Debug)]
pub enum JoinDecision {
    Allow(JoinTarget),
    Deny { kick_reason: String },
}
