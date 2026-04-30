mod file;
mod loader;

use prism::config::Config;
use serde::{Deserialize, Serialize};

#[cfg(feature = "schema")]
use schemars::JsonSchema;

pub use loader::ConfigLoader;
pub use loader::canonicalize_runtime_config;

#[cfg(feature = "schema")]
pub use loader::write_schema_file;

// API configuration types (previously in prism::config, now local)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
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
            timeout_ms: 5000,
            traffic_interval_ms: 5000,
            mock_target_addr: "127.0.0.1:25565".to_string(),
            mock_rewrite_addr: None,
            mock_connection_id_prefix: "PRSM".to_string(),
            mock_kick_reason: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    #[default]
    Http,
    Mock,
}

/// Combined config for necron-prism (prism core + api extension)
#[derive(Clone, Debug)]
pub struct NecronPrismConfig {
    pub prism: Config,
    pub api: ApiConfig,
}

#[cfg(test)]
mod test;
