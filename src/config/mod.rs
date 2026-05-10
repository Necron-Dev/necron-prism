mod canonicalize;
mod loader;

pub use acta::{LogFormat, LogLevel, LogRotation};
pub use loader::ConfigLoader;
pub use canonicalize::canonicalize_runtime_config;

#[cfg(feature = "schema")]
pub use loader::write_schema_file;

// Re-export LoggingConfig from prism (auto-generated config types)
pub use prism::config::{LoggingConfig, LogFileConfig};

use prism::config::Config;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

// API configuration types
#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct ApiConfig {
    #[default(ApiMode::Mock)]
    pub mode: ApiMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_node_key: Option<String>,
    #[default = 5000]
    pub timeout_ms: u64,
    #[default = 5000]
    pub traffic_interval_ms: u64,
    #[default("127.0.0.1:25565".to_owned())]
    pub mock_target_addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_rewrite_addr: Option<String>,
    #[default("PRSM".to_owned())]
    pub mock_connection_id_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_kick_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    Http,
    #[default]
    Mock,
}

/// Combined config for necron-prism (prism core + logging + api).
///
/// The `prism` field is flattened so TOML keys `[network]`, `[network.socket]`,
/// `[network.relay]`, `[network.buffer]`, `[motd]`, `[logging]` map directly to
/// the inner `Config`. `[api]` is a top-level section.
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