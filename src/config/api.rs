use serde::{Deserialize, Serialize};

// API defaults
const DEFAULT_API_TIMEOUT_MS: u64 = 3_000;
const DEFAULT_TRAFFIC_INTERVAL_MS: u64 = 5_000;
pub const DEFAULT_ENTRY_NODE_KEY: &str = "default";

#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub mode: ApiMode,
    pub base_url: Option<String>,
    pub bearer_token: Option<String>,
    pub entry_node_key: Option<String>,
    pub timeout_ms: u64,
    pub traffic_interval_ms: u64,
    pub mock_target_addr: String,
    pub mock_rewrite_addr: Option<String>,
    pub mock_connection_id_prefix: String,
    pub mock_kick_reason: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            mode: ApiMode::Mock,
            base_url: None,
            bearer_token: None,
            entry_node_key: None,
            timeout_ms: DEFAULT_API_TIMEOUT_MS,
            traffic_interval_ms: DEFAULT_TRAFFIC_INTERVAL_MS,
            mock_target_addr: "mc.hypixel.net:25565".to_string(),
            mock_rewrite_addr: None,
            mock_connection_id_prefix: "debug".to_string(),
            mock_kick_reason: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    Http,
    #[default]
    Mock,
}
