use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use strum::Display;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

// Network defaults
const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:25565";
const DEFAULT_FIRST_PACKET_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_KEEPALIVE_SECS: u64 = 30;
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

// Logging defaults
const DEFAULT_STATS_LOG_INTERVAL_SECS: u64 = 10;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub network: NetworkConfig,
    pub motd: MotdConfig,
    pub logging: LoggingConfig,
    pub source_path: PathBuf,
    pub requested_relay: RelayConfig,
}

#[derive(Clone, Debug, Default)]
pub struct NetworkConfig {
    pub socket: NetworkSocketConfig,
    pub relay: RelayConfig,
    pub buffer: BufferConfig,
}

#[derive(Clone, Debug)]
pub struct NetworkSocketConfig {
    pub listen_addr: String,
    pub multipath_tcp: bool,
    pub first_packet_timeout_ms: u64,
    pub tcp_nodelay: bool,
    pub tcp_keepalive: bool,
    pub keepalive_secs: Option<u64>,
    pub recv_buffer_size: Option<usize>,
    pub send_buffer_size: Option<usize>,
    pub reuse_address: bool,
    pub reuse_port: bool,
    pub listen_backlog: u32,
    pub tcp_fastopen: bool,
    pub tcp_fastopen_queue: Option<u32>,
    pub tcp_quickack: bool,
    pub ip_tos: Option<u8>,
    pub congestion_control: Option<String>,
    pub bind_interface: Option<String>,
    pub fwmark: Option<u32>,
    pub tcp_notsent_lowat: Option<u32>,
    pub so_busy_poll: Option<u32>,
}

impl Default for NetworkSocketConfig {
    fn default() -> Self {
        Self {
            listen_addr: DEFAULT_LISTEN_ADDR.to_string(),
            multipath_tcp: true,
            first_packet_timeout_ms: DEFAULT_FIRST_PACKET_TIMEOUT_MS,
            tcp_nodelay: true,
            tcp_keepalive: true,
            keepalive_secs: Some(DEFAULT_KEEPALIVE_SECS),
            recv_buffer_size: None,
            send_buffer_size: None,
            reuse_address: true,
            reuse_port: true,
            listen_backlog: DEFAULT_LISTEN_BACKLOG,
            tcp_fastopen: true,
            tcp_fastopen_queue: None,
            tcp_quickack: true,
            ip_tos: Some(DEFAULT_IP_TOS),
            congestion_control: None,
            bind_interface: None,
            fwmark: None,
            tcp_notsent_lowat: Some(DEFAULT_TCP_NOTSENT_LOWAT),
            so_busy_poll: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BufferConfig {
    pub relay_buffer_size: usize,
    pub io_uring_buffer_size: usize,
    pub splice_pipe_chunk_size: usize,
    pub packet_read_buffer_size: usize,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            relay_buffer_size: DEFAULT_RELAY_BUFFER_SIZE,
            io_uring_buffer_size: DEFAULT_IO_URING_BUFFER_SIZE,
            splice_pipe_chunk_size: DEFAULT_SPLICE_PIPE_CHUNK_SIZE,
            packet_read_buffer_size: DEFAULT_PACKET_READ_BUFFER_SIZE,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MotdConfig {
    pub mode: MotdMode,
    pub local_json: String,
    pub upstream_addr: String,
    pub protocol: MotdProtocol,
    pub ping_mode: StatusPingMode,
    pub ping_target_addr: Option<String>,
    pub upstream_ping_timeout_ms: u64,
    pub favicon: MotdFaviconConfig,
}

impl Default for MotdConfig {
    fn default() -> Self {
        Self {
            mode: MotdMode::Local,
            local_json: "{\"version\":{\"name\":\"§bnecron-prism §7status\",\"protocol\":-1},\"players\":{\"max\":100,\"online\":{online_player},\"sample\":[{\"name\":\"§7mode §8> §f{relay_mode}\",\"id\":\"00000000-0000-0000-0000-000000000001\"},{\"name\":\"§7ping §8> §b{ping_mode}\",\"id\":\"00000000-0000-0000-0000-000000000002\"},{\"name\":\"§7target §8> §f{motd_target_addr}\",\"id\":\"00000000-0000-0000-0000-000000000003\"}]},\"description\":{\"text\":\"§bnecron-prism §8» §fclean minecraft relay\\n§7online §f{online_player} §8| §7favicon §f{favicon_mode} §8| §7ping §b{ping_mode}\"}}".to_string(),
            upstream_addr: "mc.hypixel.net:25565".to_string(),
            protocol: MotdProtocol::Client,
            ping_mode: StatusPingMode::Local,
            ping_target_addr: None,
            upstream_ping_timeout_ms: DEFAULT_UPSTREAM_PING_TIMEOUT_MS,
            favicon: MotdFaviconConfig::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    pub async_enabled: bool,
    pub stats_log_interval_secs: Option<u64>,
    pub file: Option<LogFileConfig>,
}

#[derive(Clone, Debug)]
pub struct LogFileConfig {
    pub path: PathBuf,
    pub mode: LogRotation,
    pub archive_pattern: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LogRotation {
    #[default]
    None,
    Rename,
    Compress,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Pretty,
            async_enabled: true,
            stats_log_interval_secs: Some(DEFAULT_STATS_LOG_INTERVAL_SECS),
            file: None,
        }
    }
}

impl Default for LogFileConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data/logs/latest.log"),
            mode: LogRotation::Compress,
            archive_pattern: "{date}-{index}.log.gz".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MotdFaviconConfig {
    pub mode: MotdFaviconMode,
    pub path: Option<PathBuf>,
    pub target_addr: Option<String>,
}

impl Default for MotdFaviconConfig {
    fn default() -> Self {
        Self {
            mode: MotdFaviconMode::Json,
            path: None,
            target_addr: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_filter_directive(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Pretty,
    Compact,
    Json,
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

#[derive(Clone, Debug)]
pub struct RelayConfig {
    pub mode: RelayMode,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            mode: RelayMode::Async,
        }
    }
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
