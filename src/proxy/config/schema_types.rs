#![cfg_attr(not(test), allow(dead_code))]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::config_literals::CONFIG_SCHEMA_DIRECTIVE;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(description = CONFIG_SCHEMA_DIRECTIVE)]
pub struct ConfigFile {
    pub inbound: Option<InboundFileConfig>,
    pub transport: Option<TransportFileConfig>,
    pub relay: Option<RelayFileConfig>,
    pub api: Option<ApiFileConfig>,
    pub runtime: Option<RuntimeFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InboundFileConfig {
    pub listen_addr: Option<String>,
    pub first_packet_timeout_ms: Option<u64>,
    pub socket: Option<SocketOptionsFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportFileConfig {
    pub motd: Option<MotdFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RelayFileConfig {
    pub mode: Option<RelayModeLiteral>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApiFileConfig {
    pub mode: Option<ApiModeLiteral>,
    pub base_url: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout_ms: Option<u64>,
    pub traffic_interval_ms: Option<u64>,
    pub mock: Option<MockApiFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MockApiFileConfig {
    pub target_addr: Option<String>,
    pub kick_reason: Option<String>,
    pub connection_id_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdFileConfig {
    pub mode: Option<MotdModeLiteral>,
    pub json: Option<String>,
    pub upstream_addr: Option<String>,
    pub protocol: Option<MotdProtocolLiteral>,
    pub ping_mode: Option<StatusPingModeLiteral>,
    pub upstream_ping_timeout_ms: Option<u64>,
    pub status_cache_ttl_ms: Option<u64>,
    pub rewrite: Option<MotdRewriteFileConfig>,
    pub favicon: Option<MotdFaviconFileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdRewriteFileConfig {
    pub description_pattern: Option<String>,
    pub description_replacement: Option<String>,
    pub favicon_pattern: Option<String>,
    pub favicon_replacement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotdFaviconFileConfig {
    pub mode: Option<MotdFaviconModeLiteral>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFileConfig {
    pub stats_log_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SocketOptionsFileConfig {
    pub tcp_nodelay: Option<bool>,
    pub keepalive_secs: Option<u64>,
    pub recv_buffer_size: Option<usize>,
    pub send_buffer_size: Option<usize>,
    pub reuse_port: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiModeLiteral {
    Http,
    Mock,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelayModeLiteral {
    Standard,
    LinuxSplice,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MotdModeLiteral {
    Local,
    Upstream,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MotdProtocolLiteral {
    Named(MotdProtocolNamedLiteral),
    Fixed(i32),
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum MotdFaviconModeLiteral {
    #[serde(rename = "passthrough")]
    Passthrough,
    #[serde(rename = "override")]
    Override,
    #[serde(rename = "remove")]
    Remove,
}
