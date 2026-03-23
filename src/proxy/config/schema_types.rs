#![cfg_attr(not(test), allow(dead_code))]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::config_literals::CONFIG_SCHEMA_DIRECTIVE;
use super::default::{
    DEFAULT_API_TARGET_ADDR, DEFAULT_API_TIMEOUT_MS, DEFAULT_API_TRAFFIC_INTERVAL_MS,
    DEFAULT_CONNECTION_ID_PREFIX, DEFAULT_FIRST_PACKET_TIMEOUT_MS, DEFAULT_KEEPALIVE_SECS,
    DEFAULT_LISTEN_ADDR, DEFAULT_LOCAL_MOTD_JSON, DEFAULT_MOTD_STATUS_CACHE_TTL_MS,
    DEFAULT_MOTD_UPSTREAM_ADDR, DEFAULT_MOTD_UPSTREAM_PING_TIMEOUT_MS,
    DEFAULT_STATS_LOG_INTERVAL_SECS,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
#[schemars(description = CONFIG_SCHEMA_DIRECTIVE)]
pub struct ConfigFile {
    #[serde(default)]
    pub inbound: InboundFileConfig,
    #[serde(default)]
    pub transport: TransportFileConfig,
    #[serde(default)]
    pub relay: RelayFileConfig,
    #[serde(default)]
    pub api: ApiFileConfig,
    #[serde(default)]
    pub runtime: RuntimeFileConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InboundFileConfig {
    #[serde(default)]
    pub listen_addr: String,
    #[serde(default)]
    pub first_packet_timeout_ms: u64,
    #[serde(default)]
    pub socket: SocketOptionsFileConfig,
}

impl Default for InboundFileConfig {
    fn default() -> Self {
        Self {
            listen_addr: DEFAULT_LISTEN_ADDR.to_string(),
            first_packet_timeout_ms: DEFAULT_FIRST_PACKET_TIMEOUT_MS,
            socket: SocketOptionsFileConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TransportFileConfig {
    #[serde(default)]
    pub motd: MotdFileConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RelayFileConfig {
    #[serde(default)]
    pub mode: RelayModeLiteral,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiFileConfig {
    #[serde(default)]
    pub mode: ApiModeLiteral,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub timeout_ms: u64,
    #[serde(default)]
    pub traffic_interval_ms: u64,
    #[serde(default)]
    pub mock: MockApiFileConfig,
}

impl Default for ApiFileConfig {
    fn default() -> Self {
        Self {
            mode: ApiModeLiteral::default(),
            base_url: None,
            bearer_token: None,
            timeout_ms: DEFAULT_API_TIMEOUT_MS,
            traffic_interval_ms: DEFAULT_API_TRAFFIC_INTERVAL_MS,
            mock: MockApiFileConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MockApiFileConfig {
    #[serde(default)]
    pub target_addr: String,
    #[serde(default)]
    pub kick_reason: Option<String>,
    #[serde(default)]
    pub connection_id_prefix: String,
}

impl Default for MockApiFileConfig {
    fn default() -> Self {
        Self {
            target_addr: DEFAULT_API_TARGET_ADDR.to_string(),
            kick_reason: None,
            connection_id_prefix: DEFAULT_CONNECTION_ID_PREFIX.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdFileConfig {
    #[serde(default)]
    pub mode: MotdModeLiteral,
    #[serde(default)]
    pub json: String,
    #[serde(default)]
    pub upstream_addr: String,
    #[serde(default)]
    pub protocol: MotdProtocolLiteral,
    #[serde(default)]
    pub ping_mode: StatusPingModeLiteral,
    #[serde(default)]
    pub upstream_ping_timeout_ms: u64,
    #[serde(default)]
    pub status_cache_ttl_ms: u64,
    #[serde(default)]
    pub favicon: MotdFaviconFileConfig,
}

impl Default for MotdFileConfig {
    fn default() -> Self {
        Self {
            mode: MotdModeLiteral::default(),
            json: DEFAULT_LOCAL_MOTD_JSON.to_string(),
            upstream_addr: DEFAULT_MOTD_UPSTREAM_ADDR.to_string(),
            protocol: MotdProtocolLiteral::default(),
            ping_mode: StatusPingModeLiteral::default(),
            upstream_ping_timeout_ms: DEFAULT_MOTD_UPSTREAM_PING_TIMEOUT_MS,
            status_cache_ttl_ms: DEFAULT_MOTD_STATUS_CACHE_TTL_MS,
            favicon: MotdFaviconFileConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdFaviconFileConfig {
    #[serde(default)]
    pub mode: MotdFaviconModeLiteral,
    #[serde(default)]
    pub value: Option<String>,
}

impl Default for MotdFaviconFileConfig {
    fn default() -> Self {
        Self {
            mode: MotdFaviconModeLiteral::default(),
            value: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFileConfig {
    #[serde(default)]
    pub stats_log_interval_secs: u64,
}

impl Default for RuntimeFileConfig {
    fn default() -> Self {
        Self {
            stats_log_interval_secs: DEFAULT_STATS_LOG_INTERVAL_SECS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SocketOptionsFileConfig {
    #[serde(default)]
    pub tcp_nodelay: bool,
    #[serde(default)]
    pub keepalive_secs: u64,
    #[serde(default)]
    pub recv_buffer_size: Option<usize>,
    #[serde(default)]
    pub send_buffer_size: Option<usize>,
    #[serde(default)]
    pub reuse_port: bool,
}

impl Default for SocketOptionsFileConfig {
    fn default() -> Self {
        Self {
            tcp_nodelay: true,
            keepalive_secs: DEFAULT_KEEPALIVE_SECS,
            recv_buffer_size: None,
            send_buffer_size: None,
            reuse_port: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiModeLiteral {
    Http,
    #[default]
    Mock,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelayModeLiteral {
    #[default]
    Standard,
    LinuxSplice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum MotdModeLiteral {
    #[default]
    Local,
    Upstream,
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
    #[serde(rename = "remove")]
    Remove,
}

impl Default for MotdFaviconModeLiteral {
    fn default() -> Self {
        Self::Passthrough
    }
}
