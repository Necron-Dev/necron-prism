mod loader;

use prism::config::Config;
use serde::{Deserialize, Serialize};

#[cfg(feature = "schema")]
use schemars::JsonSchema;

pub use loader::ConfigLoader;
pub use loader::canonicalize_runtime_config;

#[cfg(feature = "schema")]
pub use loader::write_schema_file;

// API configuration types
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct ApiConfig {
    pub mode: ApiMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_node_key: Option<String>,
    pub timeout_ms: u64,
    pub traffic_interval_ms: u64,
    pub mock_target_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_rewrite_addr: Option<String>,
    pub mock_connection_id_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    Http,
    #[default]
    Mock,
}

/// Combined config for necron-prism (prism core + api extension).
///
/// This struct is directly deserializable from TOML. The `prism` field is
/// flattened so the TOML keys `[network]`, `[motd]`, `[logging]` map directly
/// to the inner `Config` fields, while `[api]` maps to the `ApiConfig`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct NecronPrismConfig {
    #[serde(flatten)]
    pub prism: Config,
    pub api: ApiConfig,
}

#[cfg(test)]
mod test;
