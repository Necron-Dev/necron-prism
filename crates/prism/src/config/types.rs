use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Clone, Debug)]
pub struct Config {
    pub network: NetworkConfig,
    pub motd: MotdConfig,
    pub api: ApiConfig,
    pub logging: LoggingConfig,
    pub source_path: PathBuf,
    pub requested_relay: RelayConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            motd: MotdConfig::default(),
            api: ApiConfig::default(),
            logging: LoggingConfig::default(),
            source_path: PathBuf::new(),
            requested_relay: RelayConfig::default(),
        }
    }
}

#[derive(Clone, Debug)]
#[derive(Default)]
pub struct NetworkConfig {
    pub socket: NetworkSocketConfig,
    pub relay: RelayConfig,
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
}

impl Default for NetworkSocketConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:25565".to_string(),
            multipath_tcp: true,
            first_packet_timeout_ms: 5_000,
            tcp_nodelay: true,
            tcp_keepalive: true,
            keepalive_secs: Some(30),
            recv_buffer_size: None,
            send_buffer_size: None,
            reuse_address: true,
            reuse_port: true,
            listen_backlog: 1024,
            tcp_fastopen: true,
            tcp_fastopen_queue: None,
            tcp_quickack: true,
            ip_tos: Some(0xB8),
            congestion_control: None,
            bind_interface: None,
            fwmark: None,
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
            upstream_ping_timeout_ms: 1_500,
            favicon: MotdFaviconConfig::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub mode: ApiMode,
    pub base_url: Option<String>,
    pub bearer_token: Option<String>,
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
            timeout_ms: 3_000,
            traffic_interval_ms: 5_000,
            mock_target_addr: "mc.hypixel.net:25565".to_string(),
            mock_rewrite_addr: None,
            mock_connection_id_prefix: "debug".to_string(),
            mock_kick_reason: None,
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
            stats_log_interval_secs: Some(10),
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
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Pretty,
    Compact,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    Http,
    #[default]
    Mock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdMode {
    #[default]
    Local,
    Upstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdProtocol {
    #[default]
    Client,
    #[strum(serialize = "-1")]
    NegativeOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Display, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdFaviconMode {
    #[default]
    Json,
    Path,
    Passthrough,
    Remove,
}
