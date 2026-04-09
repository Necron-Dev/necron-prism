#[cfg(feature = "schema")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use strum::Display;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
#[validate(schema(function = "Self::validate_schema"))]
pub struct Config {
    #[validate(nested)]
    pub network: NetworkConfig,
    #[validate(nested)]
    pub motd: MotdConfig,
    #[validate(nested)]
    pub api: ApiConfig,
    #[validate(nested)]
    pub logging: LoggingConfig,

    #[serde(skip)]
    pub source_path: PathBuf,
    #[serde(skip)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct NetworkConfig {
    #[validate(nested)]
    pub socket: NetworkSocketConfig,
    #[validate(nested)]
    pub relay: RelayConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            socket: NetworkSocketConfig::default(),
            relay: RelayConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct NetworkSocketConfig {
    #[validate(length(min = 1, message = "listen_addr cannot be empty"))]
    pub listen_addr: String,
    pub multipath_tcp: bool,
    pub first_packet_timeout_ms: u64,
    pub tcp_nodelay: bool,
    pub tcp_keepalive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keepalive_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recv_buffer_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_buffer_size: Option<usize>,
    pub reuse_address: bool,
    pub reuse_port: bool,
    pub listen_backlog: u32,
    pub tcp_fastopen: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_fastopen_queue: Option<u32>,
    pub tcp_quickack: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_tos: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub congestion_control: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind_interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
            ip_tos: Some(0x10),
            congestion_control: None,
            bind_interface: None,
            fwmark: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct MotdConfig {
    pub mode: MotdMode,
    #[validate(length(min = 1, message = "local_json cannot be empty"))]
    pub local_json: String,
    #[validate(length(min = 1, message = "upstream_addr cannot be empty"))]
    pub upstream_addr: String,
    pub protocol: MotdProtocol,
    pub ping_mode: StatusPingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping_target_addr: Option<String>,
    pub upstream_ping_timeout_ms: u64,
    #[validate(nested)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct ApiConfig {
    pub mode: ApiMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    pub timeout_ms: u64,
    pub traffic_interval_ms: u64,
    #[validate(length(min = 1, message = "mock_target_addr cannot be empty"))]
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
            timeout_ms: 3_000,
            traffic_interval_ms: 5_000,
            mock_target_addr: "mc.hypixel.net:25565".to_string(),
            mock_rewrite_addr: None,
            mock_connection_id_prefix: "debug".to_string(),
            mock_kick_reason: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    pub async_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats_log_interval_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub file: Option<LogFileConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct LogFileConfig {
    pub path: PathBuf,
    pub mode: LogRotation,
    pub archive_pattern: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct MotdFaviconConfig {
    pub mode: MotdFaviconMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Pretty,
    Compact,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    Http,
    #[default]
    Mock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Display)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RelayMode {
    #[default]
    Async,
    IoUring,
    Splice,
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Display)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdMode {
    #[default]
    Local,
    Upstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Display)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MotdProtocol {
    #[default]
    Client,
    #[strum(serialize = "-1")]
    NegativeOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Display)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, Display)]
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

impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        Validate::validate(self).map_err(|e| anyhow::anyhow!(e))
    }

    fn validate_schema(&self) -> Result<(), validator::ValidationError> {
        if self.api.mode == ApiMode::Http && self.api.base_url.is_none() {
            return Err(validator::ValidationError::new("api_base_url_required"));
        }
        if self.motd.favicon.mode == MotdFaviconMode::Path && self.motd.favicon.path.is_none() {
            return Err(validator::ValidationError::new("favicon_path_required"));
        }
        Ok(())
    }
}
