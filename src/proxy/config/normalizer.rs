use anyhow::Context;
use std::path::PathBuf;
use std::time::Duration;

use super::default::{
    DEFAULT_API_TARGET_ADDR, DEFAULT_CONNECTION_ID_PREFIX, DEFAULT_FIRST_PACKET_TIMEOUT_MS,
    DEFAULT_LISTEN_ADDR,
};
use super::schema_types::{
    ApiModeLiteral, ConfigFile, MotdFaviconModeLiteral, MotdProtocolLiteral,
    MotdProtocolNamedLiteral, StatusPingModeLiteral,
};
use super::types::{
    ApiConfig, ApiMode, Config, InboundConfig, MockApiConfig, MotdConfig, MotdFaviconConfig,
    MotdFaviconMode, MotdMode, MotdPingConfig, MotdProtocolMode, RelayConfig, RelayMode,
    SocketOptions, StatusPingMode, TransportConfig,
};

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
        let mock_rewrite_addr = raw
            .api
            .mock
            .rewrite_addr
            .as_deref()
            .map(|value| normalize_addr(value, 25565))
            .transpose()?
            .unwrap_or_else(|| mock_target_addr.clone());

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
                    keepalive: Some(Duration::from_secs(raw.inbound.socket.keepalive_secs)),
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
                    local_json: raw.transport.motd.json,
                    upstream_addr: raw
                        .transport
                        .motd
                        .upstream_addr
                        .as_deref()
                        .map(|value| normalize_addr(value, 25565))
                        .transpose()?,
                    protocol_mode: normalize_protocol_mode(raw.transport.motd.protocol),
                    ping_mode: normalize_ping_mode(raw.transport.motd.ping_mode),
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
                    upstream_ping_timeout_ms: Duration::from_millis(
                        raw.transport.motd.upstream_ping_timeout_ms,
                    ),
                    status_cache_ttl: Duration::from_millis(raw.transport.motd.status_cache_ttl_ms),
                    favicon: normalize_favicon_mode(raw.transport.motd.favicon, &source_path)?,
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
                timeout: Duration::from_millis(raw.api.timeout_ms),
                traffic_interval: Duration::from_millis(raw.api.traffic_interval_ms),
                mock: MockApiConfig {
                    target_addr: mock_target_addr,
                    rewrite_addr: mock_rewrite_addr,
                    connection_id_prefix: raw
                        .api
                        .mock
                        .connection_id_prefix
                        .unwrap_or_else(|| DEFAULT_CONNECTION_ID_PREFIX.to_string()),
                    kick_reason: raw.api.mock.kick_reason,
                },
            },
            stats_log_interval: raw.runtime.stats_log_interval_secs.map(Duration::from_secs),
            source_path,
        })
    }
}

fn normalize_favicon_mode(
    raw: super::schema_types::MotdFaviconFileConfig,
    source_path: &std::path::Path,
) -> anyhow::Result<MotdFaviconConfig> {
    Ok(MotdFaviconConfig {
        mode: match raw.mode {
            MotdFaviconModeLiteral::Json => MotdFaviconMode::Json,
            MotdFaviconModeLiteral::Path => MotdFaviconMode::Path,
            MotdFaviconModeLiteral::Passthrough => MotdFaviconMode::Passthrough,
            MotdFaviconModeLiteral::Remove => MotdFaviconMode::Remove,
        },
        path: raw.path.map(|value| {
            source_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(value)
        }),
        target_addr: raw
            .target_addr
            .as_deref()
            .map(|value| normalize_addr(value, 25565))
            .transpose()?,
    })
}

fn normalize_protocol_mode(value: MotdProtocolLiteral) -> MotdProtocolMode {
    match value {
        MotdProtocolLiteral::Named(MotdProtocolNamedLiteral::Client) => MotdProtocolMode::Client,
        MotdProtocolLiteral::Named(MotdProtocolNamedLiteral::NegativeOne) => {
            MotdProtocolMode::NegativeOne
        }
        MotdProtocolLiteral::Fixed(value) => MotdProtocolMode::Fixed(value),
    }
}

fn normalize_ping_mode(value: StatusPingModeLiteral) -> StatusPingMode {
    match value {
        StatusPingModeLiteral::Passthrough => StatusPingMode::Passthrough,
        StatusPingModeLiteral::ZeroMs => StatusPingMode::ZeroMs,
        StatusPingModeLiteral::UpstreamTcp => StatusPingMode::UpstreamTcp,
        StatusPingModeLiteral::Disconnect => StatusPingMode::Disconnect,
    }
}

fn normalize_addr(target_addr: &str, default_port: u16) -> anyhow::Result<String> {
    use std::net::ToSocketAddrs;

    // 如果地址没有端口，补充默认端口
    let addr_with_port = if !target_addr.contains(':')
        || (target_addr.starts_with('[') && target_addr.ends_with(']'))
    {
        format!("{target_addr}:{default_port}")
    } else {
        target_addr.to_string()
    };

    // 验证地址格式有效
    addr_with_port
        .to_socket_addrs()
        .with_context(|| format!("invalid target address: {target_addr}"))?;

    Ok(addr_with_port)
}
