use super::default::{
    DEFAULT_API_TARGET_ADDR, DEFAULT_API_TIMEOUT_MS, DEFAULT_API_TRAFFIC_INTERVAL_MS,
    DEFAULT_CONNECTION_ID_PREFIX, DEFAULT_FIRST_PACKET_TIMEOUT_MS, DEFAULT_KEEPALIVE_SECS,
    DEFAULT_LISTEN_ADDR, DEFAULT_LOCAL_MOTD_JSON, DEFAULT_MOTD_STATUS_CACHE_TTL_MS,
    DEFAULT_MOTD_UPSTREAM_PING_TIMEOUT_MS,
};
use super::schema_types::{
    ApiModeLiteral, ConfigFile, LogFormatLiteral, LogLevelLiteral, MotdFaviconModeLiteral,
    MotdProtocolLiteral, MotdProtocolNamedLiteral, StatusPingModeLiteral,
};
use super::types::{
    ApiConfig, ApiMode, Config, InboundConfig, LogFormat, LogLevel, LoggingConfig, MockApiConfig,
    MotdConfig, MotdFaviconConfig, MotdFaviconMode, MotdMode, MotdPingConfig, MotdProtocolMode,
    RelayConfig, RelayMode, RuntimeConfig, SocketOptions, StatusPingMode, TransportConfig,
};
use http::uri::Authority;
use std::path::PathBuf;
use std::time::Duration;

pub struct ConfigNormalizer;

impl ConfigNormalizer {
    pub fn new() -> Self {
        Self
    }

    pub fn normalize(&self, raw: ConfigFile, source_path: PathBuf) -> anyhow::Result<Config> {
        let mock_target_addr = normalize_addr(
            raw.api
                .mock
                .target_addr
                .as_deref()
                .unwrap_or(DEFAULT_API_TARGET_ADDR),
            25565,
        )?;

        Ok(Config {
            inbound: InboundConfig {
                listen_addr: normalize_addr(
                    raw.inbound
                        .listen_addr
                        .as_deref()
                        .unwrap_or(DEFAULT_LISTEN_ADDR),
                    25565,
                )?,
                first_packet_timeout: Duration::from_millis(
                    raw.inbound
                        .first_packet_timeout_ms
                        .unwrap_or(DEFAULT_FIRST_PACKET_TIMEOUT_MS),
                ),
                socket_options: SocketOptions {
                    tcp_nodelay: raw.inbound.socket.tcp_nodelay,
                    keepalive: Duration::from_secs(
                        raw.inbound
                            .socket
                            .keepalive_secs
                            .unwrap_or(DEFAULT_KEEPALIVE_SECS),
                    ),
                    recv_buffer_size: raw.inbound.socket.recv_buffer_size,
                    send_buffer_size: raw.inbound.socket.send_buffer_size,
                    reuse_port: raw.inbound.socket.reuse_port,
                },
            },
            transport: TransportConfig {
                motd: MotdConfig {
                    mode: match raw.transport.motd.mode {
                        super::schema_types::MotdModeLiteral::Local => MotdMode::Local,
                        super::schema_types::MotdModeLiteral::Upstream => MotdMode::Upstream,
                    },
                    local_json: raw
                        .transport
                        .motd
                        .json
                        .unwrap_or_else(|| DEFAULT_LOCAL_MOTD_JSON.to_string()),
                    upstream_addr: raw
                        .transport
                        .motd
                        .upstream_addr
                        .as_deref()
                        .map(|value| normalize_addr(value, 25565))
                        .transpose()?,
                    protocol_mode: match raw.transport.motd.protocol {
                        MotdProtocolLiteral::Named(MotdProtocolNamedLiteral::Client) => {
                            MotdProtocolMode::Client
                        }
                        MotdProtocolLiteral::Named(MotdProtocolNamedLiteral::NegativeOne) => {
                            MotdProtocolMode::NegativeOne
                        }
                        MotdProtocolLiteral::Fixed(value) => MotdProtocolMode::Fixed(value),
                    },
                    ping_mode: match raw.transport.motd.ping_mode {
                        StatusPingModeLiteral::Local => StatusPingMode::Local,
                        StatusPingModeLiteral::ZeroMs => StatusPingMode::ZeroMs,
                        StatusPingModeLiteral::Passthrough => StatusPingMode::Passthrough,
                        StatusPingModeLiteral::Disconnect => StatusPingMode::Disconnect,
                    },
                    ping: MotdPingConfig {
                        target_addr: raw
                            .transport
                            .motd
                            .ping
                            .target_addr
                            .as_deref()
                            .map(|value| normalize_addr(value, 25565))
                            .transpose()?,
                    },
                    upstream_ping_timeout: Duration::from_millis(
                        raw.transport
                            .motd
                            .upstream_ping_timeout_ms
                            .unwrap_or(DEFAULT_MOTD_UPSTREAM_PING_TIMEOUT_MS),
                    ),
                    status_cache_ttl: Duration::from_millis(
                        raw.transport
                            .motd
                            .status_cache_ttl_ms
                            .unwrap_or(DEFAULT_MOTD_STATUS_CACHE_TTL_MS),
                    ),
                    favicon: MotdFaviconConfig {
                        mode: match raw.transport.motd.favicon.mode {
                            MotdFaviconModeLiteral::Json => MotdFaviconMode::Json,
                            MotdFaviconModeLiteral::Path => MotdFaviconMode::Path,
                            MotdFaviconModeLiteral::Passthrough => MotdFaviconMode::Passthrough,
                            MotdFaviconModeLiteral::Remove => MotdFaviconMode::Remove,
                        },
                        path: raw.transport.motd.favicon.path.map(|value| {
                            source_path
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new("."))
                                .join(value)
                        }),
                        target_addr: raw
                            .transport
                            .motd
                            .favicon
                            .target_addr
                            .as_deref()
                            .map(|value| normalize_addr(value, 25565))
                            .transpose()?,
                    },
                },
            },
            relay: RelayConfig {
                mode: match raw.relay.mode {
                    super::schema_types::RelayModeLiteral::Standard => RelayMode::Standard,
                    super::schema_types::RelayModeLiteral::LinuxSplice => RelayMode::LinuxSplice,
                },
            },
            api: ApiConfig {
                mode: match raw.api.mode {
                    ApiModeLiteral::Http => ApiMode::Http,
                    ApiModeLiteral::Mock => ApiMode::Mock,
                },
                base_url: raw
                    .api
                    .base_url
                    .map(|value| value.trim_end_matches('/').to_string()),
                bearer_token: raw.api.bearer_token,
                timeout: Duration::from_millis(
                    raw.api.timeout_ms.unwrap_or(DEFAULT_API_TIMEOUT_MS),
                ),
                traffic_interval: Duration::from_millis(
                    raw.api
                        .traffic_interval_ms
                        .unwrap_or(DEFAULT_API_TRAFFIC_INTERVAL_MS),
                ),
                mock: MockApiConfig {
                    target_addr: mock_target_addr,
                    rewrite_addr: raw
                        .api
                        .mock
                        .rewrite_addr
                        .as_deref()
                        .map(|value| normalize_addr(value, 25565))
                        .transpose()?,
                    connection_id_prefix: raw
                        .api
                        .mock
                        .connection_id_prefix
                        .unwrap_or_else(|| DEFAULT_CONNECTION_ID_PREFIX.to_string()),
                    kick_reason: raw.api.mock.kick_reason,
                },
            },
            runtime: RuntimeConfig {
                stats_log_interval: raw.runtime.stats_log_interval_secs.map(Duration::from_secs),
                logging: LoggingConfig {
                    level: match raw.runtime.logging.level {
                        LogLevelLiteral::Trace => LogLevel::Trace,
                        LogLevelLiteral::Debug => LogLevel::Debug,
                        LogLevelLiteral::Info => LogLevel::Info,
                        LogLevelLiteral::Warn => LogLevel::Warn,
                        LogLevelLiteral::Error => LogLevel::Error,
                    },
                    format: match raw.runtime.logging.format {
                        LogFormatLiteral::Pretty => LogFormat::Pretty,
                        LogFormatLiteral::Compact => LogFormat::Compact,
                        LogFormatLiteral::Json => LogFormat::Json,
                    },
                    async_enabled: raw.runtime.logging.async_enabled,
                },
            },
            source_path,
        })
    }
}

fn normalize_addr(target_addr: &str, default_port: u16) -> anyhow::Result<String> {
    let authority = target_addr
        .parse::<Authority>()
        .map_err(|_| anyhow::anyhow!("invalid address"))?;

    let host = authority.host();
    let port = authority.port_u16().unwrap_or(default_port);

    if host.contains(':') && !host.starts_with('[') {
        Ok(format!("[{}]:{}", host, port))
    } else {
        Ok(format!("{}:{}", host, port))
    }
}
