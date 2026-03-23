use std::net::Ipv6Addr;
use std::path::PathBuf;
use std::time::Duration;

use regex::Regex;

use super::schema_types::{
    ApiModeLiteral, ConfigFile, MotdFaviconModeLiteral, MotdProtocolLiteral,
    MotdProtocolNamedLiteral, StatusPingModeLiteral,
};
use super::types::{
    ApiConfig, ApiMode, Config, InboundConfig, MockApiConfig, MotdConfig, MotdFaviconMode,
    MotdMode, MotdProtocolMode, MotdRewrite, RelayConfig, RelayMode, SocketOptions, StatusPingMode,
    TransportConfig,
};

pub struct ConfigNormalizer;

impl ConfigNormalizer {
    pub fn new() -> Self {
        Self
    }

    pub fn normalize(&self, raw: ConfigFile, source_path: PathBuf) -> Result<Config, String> {
        Ok(Config {
            inbound: InboundConfig {
                listen_addr: raw.inbound.listen_addr,
                first_packet_timeout: Duration::from_millis(raw.inbound.first_packet_timeout_ms),
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
                    local_json: Some(raw.transport.motd.json),
                    upstream_addr: Some(normalize_target_addr(
                        &raw.transport.motd.upstream_addr,
                        25565,
                    )?),
                    protocol_mode: normalize_protocol_mode(raw.transport.motd.protocol),
                    ping_mode: normalize_ping_mode(raw.transport.motd.ping_mode),
                    upstream_ping_timeout: Duration::from_millis(
                        raw.transport.motd.upstream_ping_timeout_ms,
                    ),
                    status_cache_ttl: Duration::from_millis(raw.transport.motd.status_cache_ttl_ms),
                    rewrite: normalize_motd_rewrite(raw.transport.motd.rewrite)?,
                    favicon: normalize_favicon_mode(raw.transport.motd.favicon),
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
                    target_addr: normalize_target_addr(&raw.api.mock.target_addr, 25565)?,
                    kick_reason: raw.api.mock.kick_reason,
                    connection_id_prefix: raw.api.mock.connection_id_prefix,
                },
            },
            stats_log_interval: Some(Duration::from_secs(raw.runtime.stats_log_interval_secs)),
            source_path,
        })
    }
}

fn normalize_motd_rewrite(
    raw: super::schema_types::MotdRewriteFileConfig,
) -> Result<Option<MotdRewrite>, String> {
    let description_pattern = compile_regex(raw.description_pattern.as_deref())?;
    let favicon_pattern = compile_regex(raw.favicon_pattern.as_deref())?;

    if description_pattern.is_none()
        && raw.description_replacement.is_none()
        && favicon_pattern.is_none()
        && raw.favicon_replacement.is_none()
    {
        return Ok(None);
    }

    Ok(Some(MotdRewrite {
        description_pattern,
        description_replacement: raw.description_replacement,
        favicon_pattern,
        favicon_replacement: raw.favicon_replacement,
    }))
}

fn compile_regex(pattern: Option<&str>) -> Result<Option<Regex>, String> {
    match pattern {
        Some(pattern) => Regex::new(pattern)
            .map(Some)
            .map_err(|error| format!("invalid regex '{pattern}': {error}")),
        None => Ok(None),
    }
}

fn normalize_favicon_mode(raw: super::schema_types::MotdFaviconFileConfig) -> MotdFaviconMode {
    match raw.mode {
        MotdFaviconModeLiteral::Passthrough => MotdFaviconMode::Passthrough,
        MotdFaviconModeLiteral::Remove => MotdFaviconMode::Remove,
        MotdFaviconModeLiteral::Override => {
            MotdFaviconMode::Override(raw.value.unwrap_or_default())
        }
    }
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

fn parse_target_port(target_addr: &str, default_port: u16) -> Result<u16, String> {
    if let Some(stripped) = target_addr.strip_prefix('[') {
        let (host, port) = stripped
            .split_once(']')
            .ok_or_else(|| format!("target address is missing a closing bracket: {target_addr}"))?;
        if host.is_empty() {
            return Err(format!("target address is missing a host: {target_addr}"));
        }

        let port = port.strip_prefix(':').unwrap_or("");
        if port.is_empty() {
            return Ok(default_port);
        }
        return parse_port(port, "target port");
    }

    if target_addr.parse::<Ipv6Addr>().is_ok() {
        return Ok(default_port);
    }

    match target_addr.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && !port.is_empty() => {
            parse_port(port, "target port")
        }
        _ => Ok(default_port),
    }
}

fn normalize_target_addr(target_addr: &str, default_port: u16) -> Result<String, String> {
    let port = parse_target_port(target_addr, default_port)?;

    if let Some(stripped) = target_addr.strip_prefix('[') {
        let (host, _) = stripped
            .split_once(']')
            .ok_or_else(|| format!("target address is missing a closing bracket: {target_addr}"))?;
        if host.is_empty() {
            return Err(format!("target address is missing a host: {target_addr}"));
        }
        return Ok(format!("[{host}]:{port}"));
    }

    if target_addr.parse::<Ipv6Addr>().is_ok() {
        return Ok(format!("[{target_addr}]:{port}"));
    }

    match target_addr.rsplit_once(':') {
        Some((host, existing_port)) if !host.is_empty() && !existing_port.is_empty() => {
            Ok(target_addr.to_string())
        }
        _ => Ok(format!("{target_addr}:{port}")),
    }
}

fn parse_port(value: &str, field_name: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| format!("invalid {field_name}: {value}"))
}
