#![cfg_attr(not(test), allow(dead_code))]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub inbound: InboundFileConfig,
    #[serde(default)]
    pub transport: TransportFileConfig,
    #[serde(default)]
    pub relay: RelayFileConfig,
    pub api: ApiFileConfig,
    #[serde(default)]
    pub runtime: Option<RuntimeFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InboundFileConfig {
    pub listen_addr: String,
    #[serde(default)]
    pub first_packet_timeout_ms: Option<u64>,
    #[serde(default)]
    pub socket: Option<SocketOptionsFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportFileConfig {
    #[serde(default)]
    pub motd: MotdFileConfig,
}

impl Default for TransportFileConfig {
    fn default() -> Self {
        Self {
            motd: MotdFileConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelayFileConfig {
    #[serde(default)]
    pub mode: RelayModeLiteral,
}

impl Default for RelayFileConfig {
    fn default() -> Self {
        Self {
            mode: RelayModeLiteral::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiFileConfig {
    pub mode: ApiModeLiteral,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub traffic_interval_ms: Option<u64>,
    #[serde(default)]
    pub mock: Option<MockApiFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MockApiFileConfig {
    #[serde(default)]
    pub target_addr: Option<String>,
    #[serde(default)]
    pub kick_reason: Option<String>,
    #[serde(default)]
    pub connection_id_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdFileConfig {
    #[serde(default)]
    pub mode: MotdModeLiteral,
    #[serde(default)]
    pub json: Option<String>,
    #[serde(default)]
    pub upstream_addr: Option<String>,
    #[serde(default)]
    pub protocol: MotdProtocolLiteral,
    #[serde(default)]
    pub ping_mode: StatusPingModeLiteral,
    #[serde(default)]
    pub upstream_ping_timeout_ms: Option<u64>,
    #[serde(default)]
    pub status_cache_ttl_ms: Option<u64>,
    #[serde(default)]
    pub rewrite: Option<MotdRewriteFileConfig>,
    #[serde(default)]
    pub favicon: Option<MotdFaviconFileConfig>,
}

impl Default for MotdFileConfig {
    fn default() -> Self {
        Self {
            mode: MotdModeLiteral::default(),
            json: None,
            upstream_addr: None,
            protocol: MotdProtocolLiteral::default(),
            ping_mode: StatusPingModeLiteral::default(),
            upstream_ping_timeout_ms: None,
            status_cache_ttl_ms: None,
            rewrite: None,
            favicon: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdRewriteFileConfig {
    #[serde(default)]
    pub description_pattern: Option<String>,
    #[serde(default)]
    pub description_replacement: Option<String>,
    #[serde(default)]
    pub favicon_pattern: Option<String>,
    #[serde(default)]
    pub favicon_replacement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdFaviconFileConfig {
    pub mode: MotdFaviconModeLiteral,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFileConfig {
    #[serde(default)]
    pub stats_log_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SocketOptionsFileConfig {
    #[serde(default)]
    pub tcp_nodelay: Option<bool>,
    #[serde(default)]
    pub keepalive_secs: Option<u64>,
    #[serde(default)]
    pub recv_buffer_size: Option<usize>,
    #[serde(default)]
    pub send_buffer_size: Option<usize>,
    #[serde(default)]
    pub reuse_port: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiModeLiteral {
    Http,
    Mock,
}

impl Default for ApiModeLiteral {
    fn default() -> Self {
        Self::Mock
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelayModeLiteral {
    Standard,
    LinuxSplice,
}

impl Default for RelayModeLiteral {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MotdModeLiteral {
    Local,
    Upstream,
}

impl Default for MotdModeLiteral {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MotdProtocolLiteral {
    Named(MotdProtocolNamedLiteral),
    Fixed(i32),
}

impl Default for MotdProtocolLiteral {
    fn default() -> Self {
        Self::Named(MotdProtocolNamedLiteral::Client)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum MotdProtocolNamedLiteral {
    #[serde(rename = "client")]
    Client,
    #[serde(rename = "-1")]
    NegativeOne,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum StatusPingModeLiteral {
    #[serde(rename = "passthrough")]
    Passthrough,
    #[serde(rename = "0ms")]
    ZeroMs,
    #[serde(rename = "upstream_tcp")]
    UpstreamTcp,
    #[serde(rename = "disconnect")]
    Disconnect,
}

impl Default for StatusPingModeLiteral {
    fn default() -> Self {
        Self::Passthrough
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum MotdFaviconModeLiteral {
    #[serde(rename = "passthrough")]
    Passthrough,
    #[serde(rename = "override")]
    Override,
    #[serde(rename = "remove")]
    Remove,
}

impl Default for MotdFaviconModeLiteral {
    fn default() -> Self {
        Self::Passthrough
    }
}
