#[derive(Clone, Debug)]
pub struct JoinTarget {
    pub target_addr: String,
    pub rewrite_addr: Option<String>,
    pub connection_id: String,
}

#[derive(Clone, Debug)]
pub enum JoinDecision {
    Allow(JoinTarget),
    Deny { kick_reason: String },
}
