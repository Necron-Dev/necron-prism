use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::path::PathBuf;
use strum::Display;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

// Logging defaults
const DEFAULT_STATS_LOG_INTERVAL_SECS: u64 = 10;
pub use acta::{LogFormat, LogLevel, LogRotation};

// Network defaults
const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:25565";
const DEFAULT_FIRST_PACKET_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_KEEPALIVE_SECS: u64 = 30;
const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 5;
const DEFAULT_KEEPALIVE_RETRIES: u32 = 3;

const DEFAULT_LISTEN_BACKLOG: u32 = 1024;
const DEFAULT_IP_TOS: u8 = 0xB8;
const DEFAULT_TCP_NOTSENT_LOWAT: u32 = 16384;

// Buffer defaults
const DEFAULT_RELAY_BUFFER_SIZE: usize = 64 * 1024;
const DEFAULT_IO_URING_BUFFER_SIZE: usize = 64 * 1024;
const DEFAULT_SPLICE_PIPE_CHUNK_SIZE: usize = 64 * 1024;
const DEFAULT_PACKET_READ_BUFFER_SIZE: usize = 16 * 1024;

// MOTD defaults
const DEFAULT_UPSTREAM_PING_TIMEOUT_MS: u64 = 1_500;
const DEFAULT_LOCAL_JSON: &str = r#"{"version":{"name":"\u00a7bnecron-prism \u00a77status","protocol":-1},"players":{"max":100,"online":{online_player},"sample":[{"name":"\u00a77mode \u00a78> \u00a7f{relay_mode}","id":"00000000-0000-0000-0000-000000000001"},{"name":"\u00a77ping \u00a78> \u00a7b{ping_mode}","id":"00000000-0000-0000-0000-000000000002"},{"name":"\u00a77target \u00a78> \u00a7f{motd_target_addr}","id":"00000000-0000-0000-0000-000000000003"}]},"description":{"text":"\u00a7bnecron-prism \u00a78\u00bb \u00a7fclean minecraft relay\n\u00a77online \u00a7f{online_player} \u00a78| \u00a77favicon \u00a7f{favicon_mode} \u00a78| \u00a77ping \u00a7b{ping_mode}"}}"#;
const DEFAULT_UPSTREAM_ADDR: &str = "mc.hypixel.net:25565";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct Config {
    pub network: NetworkConfig,
    pub motd: MotdConfig,
    #[cfg_attr(feature = "schema", schemars(skip))]
    pub logging: LoggingConfig,
    #[serde(skip)]
    pub source_path: PathBuf,
    #[serde(skip)]
    pub requested_relay: RelayConfig,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct NetworkConfig {
    pub socket: NetworkSocketConfig,
    pub relay: RelayConfig,
    pub buffer: BufferConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct NetworkSocketConfig {
    #[default(DEFAULT_LISTEN_ADDR.to_owned())]
    pub listen_addr: String,
    #[default = true]
    pub multipath_tcp: bool,
    #[default(DEFAULT_FIRST_PACKET_TIMEOUT_MS)]
    pub first_packet_timeout_ms: u64,
    #[default = true]
    pub tcp_nodelay: bool,
    #[default = true]
    pub tcp_keepalive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[default(Some(DEFAULT_KEEPALIVE_SECS))]
    pub keepalive_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[default(Some(DEFAULT_KEEPALIVE_INTERVAL_SECS))]
    pub keepalive_interval_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[default(Some(DEFAULT_KEEPALIVE_RETRIES))]
    pub keepalive_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recv_buffer_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_buffer_size: Option<usize>,
    #[default = true]
    pub reuse_address: bool,
    #[default = true]
    pub reuse_port: bool,
    #[default(DEFAULT_LISTEN_BACKLOG)]
    pub listen_backlog: u32,
    #[default = true]
    pub tcp_fastopen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_fastopen_queue: Option<u32>,
    #[default = true]
    pub tcp_quickack: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[default(Some(DEFAULT_IP_TOS))]
    pub ip_tos: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub congestion_control: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind_interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fwmark: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[default(Some(DEFAULT_TCP_NOTSENT_LOWAT))]
    pub tcp_notsent_lowat: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub so_busy_poll: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct BufferConfig {
    #[default(DEFAULT_RELAY_BUFFER_SIZE)]
    pub relay_buffer_size: usize,
    #[default(DEFAULT_IO_URING_BUFFER_SIZE)]
    pub io_uring_buffer_size: usize,
    #[default(DEFAULT_SPLICE_PIPE_CHUNK_SIZE)]
    pub splice_pipe_chunk_size: usize,
    #[default(DEFAULT_PACKET_READ_BUFFER_SIZE)]
    pub packet_read_buffer_size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct MotdConfig {
    #[default(MotdMode::Local)]
    pub mode: MotdMode,
    // https://minecraft.wiki/w/Java_Edition_protocol/Server_List_Ping#Status_Response
    #[default(DEFAULT_LOCAL_JSON.to_owned())]
    pub local_json: String,
    #[default(DEFAULT_UPSTREAM_ADDR.to_owned())]
    pub upstream_addr: String,
    #[default(MotdProtocol::Client)]
    pub protocol: MotdProtocol,
    #[default(StatusPingMode::Local)]
    pub ping_mode: StatusPingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping_target_addr: Option<String>,
    #[default(DEFAULT_UPSTREAM_PING_TIMEOUT_MS)]
    pub upstream_ping_timeout_ms: u64,
    #[default(MotdFaviconConfig::default())]
    pub favicon: MotdFaviconConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[serde(default)]
pub struct LoggingConfig {
    #[default(LogLevel::Info)]
    pub level: LogLevel,
    #[default(LogFormat::Compact)]
    pub format: LogFormat,
    #[default = true]
    pub async_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[default(Some(DEFAULT_STATS_LOG_INTERVAL_SECS))]
    pub stats_log_interval_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<LogFileConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[serde(default)]
pub struct LogFileConfig {
    #[default(PathBuf::from("data/logs/latest.log"))]
    pub path: PathBuf,
    #[default(LogRotation::Compress)]
    pub mode: LogRotation,
}

#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct MotdFaviconConfig {
    #[default(MotdFaviconMode::Json)]
    pub mode: MotdFaviconMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_addr: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RelayMode {
    #[default]
    Async,
    IoUring,
    Splice,
}

#[derive(Clone, Debug, Serialize, Deserialize, SmartDefault)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct RelayConfig {
    #[default(RelayMode::Async)]
    pub mode: RelayMode,
}

impl RelayConfig {
    pub fn label(&self) -> &'static str {
        match self.mode {
            RelayMode::Async => "async",
            RelayMode::IoUring => "io_uring",
            RelayMode::Splice => "splice",
        }
    }

    pub fn is_io_uring(&self) -> bool {
        matches!(self.mode, RelayMode::IoUring)
    }

    pub fn is_splice(&self) -> bool {
        matches!(self.mode, RelayMode::Splice)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdMode {
    #[default]
    Local,
    Upstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdProtocol {
    #[default]
    Client,
    #[strum(serialize = "-1")]
    NegativeOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StatusPingMode {
    #[default]
    Local,
    #[strum(serialize = "0ms")]
    ZeroMs,
    Passthrough,
    Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdFaviconMode {
    #[default]
    Json,
    Path,
    Passthrough,
    Remove,
}
