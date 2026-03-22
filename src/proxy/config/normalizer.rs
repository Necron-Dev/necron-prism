use std::net::Ipv6Addr;
use std::path::PathBuf;
use std::time::Duration;

use regex::Regex;

use super::schema_types::{
    ApiFileConfig, ApiModeLiteral, ConfigFile, InboundFileConfig, MockApiFileConfig,
    MotdFaviconFileConfig, MotdFaviconModeLiteral, MotdFileConfig, MotdModeLiteral,
    MotdProtocolLiteral, MotdProtocolNamedLiteral, MotdRewriteFileConfig, RelayFileConfig,
    RelayModeLiteral, SocketOptionsFileConfig, StatusPingModeLiteral,
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
            inbound: normalize_inbound(raw.inbound),
            transport: TransportConfig {
                motd: normalize_motd(raw.transport.motd)?,
            },
            relay: normalize_relay(raw.relay),
            api: normalize_api(raw.api)?,
            stats_log_interval: raw
                .runtime
                .and_then(|runtime| runtime.stats_log_interval_secs)
                .map(Duration::from_secs)
                .or(Some(Duration::from_secs(10))),
            source_path,
        })
    }
}

fn normalize_inbound(raw: InboundFileConfig) -> InboundConfig {
    InboundConfig {
        listen_addr: raw.listen_addr,
        first_packet_timeout: Duration::from_millis(raw.first_packet_timeout_ms.unwrap_or(5_000)),
        socket_options: normalize_socket_options(raw.socket),
    }
}

fn normalize_api(raw: ApiFileConfig) -> Result<ApiConfig, String> {
    Ok(ApiConfig {
        mode: match raw.mode {
            ApiModeLiteral::Http => ApiMode::Http,
            ApiModeLiteral::Mock => ApiMode::Mock,
        },
        base_url: raw
            .base_url
            .map(|value| value.trim_end_matches('/').to_string()),
        bearer_token: raw.bearer_token,
        timeout: Duration::from_millis(raw.timeout_ms.unwrap_or(3_000)),
        traffic_interval: Duration::from_millis(raw.traffic_interval_ms.unwrap_or(5_000)),
        mock: normalize_mock_api(raw.mock)?,
    })
}

fn normalize_mock_api(raw: Option<MockApiFileConfig>) -> Result<MockApiConfig, String> {
    let raw = raw.unwrap_or(MockApiFileConfig {
        target_addr: None,
        kick_reason: None,
        connection_id_prefix: None,
    });

    Ok(MockApiConfig {
        target_addr: normalize_target_addr(
            raw.target_addr.as_deref().unwrap_or("127.0.0.1:25565"),
            25565,
        )?,
        kick_reason: raw.kick_reason,
        connection_id_prefix: raw
            .connection_id_prefix
            .unwrap_or_else(|| "mock".to_string()),
    })
}

fn normalize_relay(raw: RelayFileConfig) -> RelayConfig {
    RelayConfig {
        mode: match raw.mode {
            RelayModeLiteral::Standard => RelayMode::Standard,
            RelayModeLiteral::LinuxSplice => RelayMode::LinuxSplice,
        },
    }
}

fn normalize_motd(raw: MotdFileConfig) -> Result<MotdConfig, String> {
    Ok(MotdConfig {
        mode: match raw.mode {
            MotdModeLiteral::Local => MotdMode::Local,
            MotdModeLiteral::Upstream => MotdMode::Upstream,
        },
        local_json: raw.json,
        upstream_addr: raw
            .upstream_addr
            .as_deref()
            .map(|addr| normalize_target_addr(addr, 25565))
            .transpose()?,
        protocol_mode: normalize_protocol_mode(raw.protocol),
        ping_mode: normalize_ping_mode(raw.ping_mode),
        upstream_ping_timeout: Duration::from_millis(raw.upstream_ping_timeout_ms.unwrap_or(1500)),
        status_cache_ttl: Duration::from_millis(raw.status_cache_ttl_ms.unwrap_or(1000)),
        rewrite: normalize_motd_rewrite(raw.rewrite)?,
        favicon: normalize_favicon_mode(raw.favicon),
    })
}

fn normalize_motd_rewrite(
    raw: Option<MotdRewriteFileConfig>,
) -> Result<Option<MotdRewrite>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };

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

fn normalize_favicon_mode(raw: Option<MotdFaviconFileConfig>) -> MotdFaviconMode {
    match raw.as_ref().map(|value| value.mode) {
        Some(MotdFaviconModeLiteral::Passthrough) | None => MotdFaviconMode::Passthrough,
        Some(MotdFaviconModeLiteral::Remove) => MotdFaviconMode::Remove,
        Some(MotdFaviconModeLiteral::Override) => {
            MotdFaviconMode::Override(raw.and_then(|favicon| favicon.value).unwrap_or_default())
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

fn normalize_socket_options(raw: Option<SocketOptionsFileConfig>) -> SocketOptions {
    let raw = raw.unwrap_or(SocketOptionsFileConfig {
        tcp_nodelay: None,
        keepalive_secs: None,
        recv_buffer_size: None,
        send_buffer_size: None,
        reuse_port: None,
    });
    let defaults = SocketOptions::default();
    SocketOptions {
        tcp_nodelay: raw.tcp_nodelay.unwrap_or(defaults.tcp_nodelay),
        keepalive: raw
            .keepalive_secs
            .map(Duration::from_secs)
            .or(defaults.keepalive),
        recv_buffer_size: raw.recv_buffer_size,
        send_buffer_size: raw.send_buffer_size,
        reuse_port: raw.reuse_port.unwrap_or(defaults.reuse_port),
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
