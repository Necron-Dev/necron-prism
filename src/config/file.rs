use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[cfg(feature = "schema")]
use schemars::JsonSchema;

use crate::config::{ApiConfig, ApiMode, NecronPrismConfig};
use prism::config::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
#[derive(Default)]
pub struct FileConfig {
    pub network: FileNetworkConfig,
    pub motd: FileMotdConfig,
    pub api: FileApiConfig,
    pub logging: FileLoggingConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
#[derive(Default)]
pub struct FileNetworkConfig {
    pub socket: FileNetworkSocketConfig,
    pub relay: FileRelayConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct FileNetworkSocketConfig {
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

impl Default for FileNetworkSocketConfig {
    fn default() -> Self {
        let defaults = NetworkSocketConfig::default();
        Self {
            listen_addr: defaults.listen_addr,
            multipath_tcp: defaults.multipath_tcp,
            first_packet_timeout_ms: defaults.first_packet_timeout_ms,
            tcp_nodelay: defaults.tcp_nodelay,
            tcp_keepalive: defaults.tcp_keepalive,
            keepalive_secs: defaults.keepalive_secs,
            recv_buffer_size: defaults.recv_buffer_size,
            send_buffer_size: defaults.send_buffer_size,
            reuse_address: defaults.reuse_address,
            reuse_port: defaults.reuse_port,
            listen_backlog: defaults.listen_backlog,
            tcp_fastopen: defaults.tcp_fastopen,
            tcp_fastopen_queue: defaults.tcp_fastopen_queue,
            tcp_quickack: defaults.tcp_quickack,
            ip_tos: defaults.ip_tos,
            congestion_control: defaults.congestion_control,
            bind_interface: defaults.bind_interface,
            fwmark: defaults.fwmark,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct FileMotdConfig {
    pub mode: MotdMode,
    pub local_json: String,
    pub upstream_addr: String,
    pub protocol: MotdProtocol,
    pub ping_mode: StatusPingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ping_target_addr: Option<String>,
    pub upstream_ping_timeout_ms: u64,
    pub favicon: FileMotdFaviconConfig,
}

impl Default for FileMotdConfig {
    fn default() -> Self {
        let defaults = MotdConfig::default();
        Self {
            mode: defaults.mode,
            local_json: defaults.local_json,
            upstream_addr: defaults.upstream_addr,
            protocol: defaults.protocol,
            ping_mode: defaults.ping_mode,
            ping_target_addr: defaults.ping_target_addr,
            upstream_ping_timeout_ms: defaults.upstream_ping_timeout_ms,
            favicon: FileMotdFaviconConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct FileApiConfig {
    pub mode: ApiMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
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

impl Default for FileApiConfig {
    fn default() -> Self {
        let defaults = ApiConfig::default();
        Self {
            mode: defaults.mode,
            base_url: defaults.base_url,
            bearer_token: defaults.bearer_token,
            entry_node_key: defaults.entry_node_key,
            timeout_ms: defaults.timeout_ms,
            traffic_interval_ms: defaults.traffic_interval_ms,
            mock_target_addr: defaults.mock_target_addr,
            mock_rewrite_addr: defaults.mock_rewrite_addr,
            mock_connection_id_prefix: defaults.mock_connection_id_prefix,
            mock_kick_reason: defaults.mock_kick_reason,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct FileLoggingConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    pub async_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats_log_interval_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileLogFileConfig>,
}

impl Default for FileLoggingConfig {
    fn default() -> Self {
        let defaults = LoggingConfig::default();
        Self {
            level: defaults.level,
            format: defaults.format,
            async_enabled: defaults.async_enabled,
            stats_log_interval_secs: defaults.stats_log_interval_secs,
            file: defaults.file.map(|f| FileLogFileConfig {
                path: f.path,
                mode: f.mode,
                archive_pattern: f.archive_pattern,
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct FileLogFileConfig {
    pub path: PathBuf,
    pub mode: LogRotation,
    pub archive_pattern: String,
}

impl Default for FileLogFileConfig {
    fn default() -> Self {
        let defaults = LogFileConfig::default();
        Self {
            path: defaults.path,
            mode: defaults.mode,
            archive_pattern: defaults.archive_pattern,
        }
    }
}

impl From<FileLogFileConfig> for LogFileConfig {
    fn from(value: FileLogFileConfig) -> Self {
        Self {
            path: value.path,
            mode: value.mode,
            archive_pattern: value.archive_pattern,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct FileMotdFaviconConfig {
    pub mode: MotdFaviconMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_addr: Option<String>,
}

impl Default for FileMotdFaviconConfig {
    fn default() -> Self {
        let defaults = MotdFaviconConfig::default();
        Self {
            mode: defaults.mode,
            path: defaults.path,
            target_addr: defaults.target_addr,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(default)]
pub struct FileRelayConfig {
    pub mode: RelayMode,
}

impl Default for FileRelayConfig {
    fn default() -> Self {
        Self {
            mode: RelayMode::Async,
        }
    }
}

impl From<FileConfig> for NecronPrismConfig {
    fn from(file: FileConfig) -> Self {
        Self {
            prism: Config {
                network: NetworkConfig {
                    socket: NetworkSocketConfig {
                        listen_addr: file.network.socket.listen_addr,
                        multipath_tcp: file.network.socket.multipath_tcp,
                        first_packet_timeout_ms: file.network.socket.first_packet_timeout_ms,
                        tcp_nodelay: file.network.socket.tcp_nodelay,
                        tcp_keepalive: file.network.socket.tcp_keepalive,
                        keepalive_secs: file.network.socket.keepalive_secs,
                        recv_buffer_size: file.network.socket.recv_buffer_size,
                        send_buffer_size: file.network.socket.send_buffer_size,
                        reuse_address: file.network.socket.reuse_address,
                        reuse_port: file.network.socket.reuse_port,
                        listen_backlog: file.network.socket.listen_backlog,
                        tcp_fastopen: file.network.socket.tcp_fastopen,
                        tcp_fastopen_queue: file.network.socket.tcp_fastopen_queue,
                        tcp_quickack: file.network.socket.tcp_quickack,
                        ip_tos: file.network.socket.ip_tos,
                        congestion_control: file.network.socket.congestion_control,
                        bind_interface: file.network.socket.bind_interface,
                        fwmark: file.network.socket.fwmark,
                        tcp_notsent_lowat: None,
                        so_busy_poll: None,
                    },
                    relay: RelayConfig {
                        mode: file.network.relay.mode,
                    },
                    buffer: BufferConfig::default(),
                },
                motd: MotdConfig {
                    mode: file.motd.mode,
                    local_json: file.motd.local_json,
                    upstream_addr: file.motd.upstream_addr,
                    protocol: file.motd.protocol,
                    ping_mode: file.motd.ping_mode,
                    ping_target_addr: file.motd.ping_target_addr,
                    upstream_ping_timeout_ms: file.motd.upstream_ping_timeout_ms,
                    favicon: MotdFaviconConfig {
                        mode: file.motd.favicon.mode,
                        path: file.motd.favicon.path,
                        target_addr: file.motd.favicon.target_addr,
                    },
                },
                logging: LoggingConfig {
                    level: file.logging.level,
                    format: file.logging.format,
                    async_enabled: file.logging.async_enabled,
                    stats_log_interval_secs: file.logging.stats_log_interval_secs,
                    file: file.logging.file.map(LogFileConfig::from),
                },
                source_path: PathBuf::new(),
                requested_relay: RelayConfig::default(),
            },
            api: ApiConfig {
                mode: file.api.mode,
                base_url: file.api.base_url,
                bearer_token: file.api.bearer_token,
                entry_node_key: file.api.entry_node_key,
                timeout_ms: file.api.timeout_ms,
                traffic_interval_ms: file.api.traffic_interval_ms,
                mock_target_addr: file.api.mock_target_addr,
                mock_rewrite_addr: file.api.mock_rewrite_addr,
                mock_connection_id_prefix: file.api.mock_connection_id_prefix,
                mock_kick_reason: file.api.mock_kick_reason,
            },
        }
    }
}
