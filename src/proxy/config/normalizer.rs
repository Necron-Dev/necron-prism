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
        let inbound = raw
            .inbound
            .ok_or_else(|| "missing [inbound] config after defaults applied".to_string())?;
        let transport = raw
            .transport
            .ok_or_else(|| "missing [transport] config after defaults applied".to_string())?;
        let relay = raw
            .relay
            .ok_or_else(|| "missing [relay] config after defaults applied".to_string())?;
        let api = raw
            .api
            .ok_or_else(|| "missing [api] config after defaults applied".to_string())?;

        Ok(Config {
            inbound: normalize_inbound(inbound)?,
            transport: TransportConfig {
                motd: normalize_motd(transport.motd.ok_or_else(|| {
                    "missing [transport.motd] config after defaults applied".to_string()
                })?)?,
            },
            relay: normalize_relay(relay)?,
            api: normalize_api(api)?,
            stats_log_interval: raw
                .runtime
                .and_then(|runtime| runtime.stats_log_interval_secs)
                .map(Duration::from_secs),
            source_path,
        })
    }
}

fn normalize_inbound(raw: InboundFileConfig) -> Result<InboundConfig, String> {
    Ok(InboundConfig {
        listen_addr: raw
            .listen_addr
            .ok_or_else(|| "missing inbound.listen_addr after defaults applied".to_string())?,
        first_packet_timeout: Duration::from_millis(raw.first_packet_timeout_ms.ok_or_else(
            || "missing inbound.first_packet_timeout_ms after defaults applied".to_string(),
        )?),
        socket_options: normalize_socket_options(
            raw.socket.ok_or_else(|| {
                "missing inbound.socket config after defaults applied".to_string()
            })?,
        )?,
    })
}

fn normalize_api(raw: ApiFileConfig) -> Result<ApiConfig, String> {
    Ok(ApiConfig {
        mode: match raw
            .mode
            .ok_or_else(|| "missing api.mode after defaults applied".to_string())?
        {
            ApiModeLiteral::Http => ApiMode::Http,
            ApiModeLiteral::Mock => ApiMode::Mock,
        },
        base_url: raw
            .base_url
            .map(|value| value.trim_end_matches('/').to_string()),
        bearer_token: raw.bearer_token,
        timeout: Duration::from_millis(
            raw.timeout_ms
                .ok_or_else(|| "missing api.timeout_ms after defaults applied".to_string())?,
        ),
        traffic_interval: Duration::from_millis(
            raw.traffic_interval_ms.ok_or_else(|| {
                "missing api.traffic_interval_ms after defaults applied".to_string()
            })?,
        ),
        mock: normalize_mock_api(
            raw.mock
                .ok_or_else(|| "missing api.mock config after defaults applied".to_string())?,
        )?,
    })
}

fn normalize_mock_api(raw: MockApiFileConfig) -> Result<MockApiConfig, String> {
    Ok(MockApiConfig {
        target_addr: normalize_target_addr(
            raw.target_addr
                .as_deref()
                .ok_or_else(|| "missing api.mock.target_addr after defaults applied".to_string())?,
            25565,
        )?,
        kick_reason: raw.kick_reason,
        connection_id_prefix: raw.connection_id_prefix.ok_or_else(|| {
            "missing api.mock.connection_id_prefix after defaults applied".to_string()
        })?,
    })
}

fn normalize_relay(raw: RelayFileConfig) -> Result<RelayConfig, String> {
    Ok(RelayConfig {
        mode: match raw
            .mode
            .ok_or_else(|| "missing relay.mode after defaults applied".to_string())?
        {
            RelayModeLiteral::Standard => RelayMode::Standard,
            RelayModeLiteral::LinuxSplice => RelayMode::LinuxSplice,
        },
    })
}

fn normalize_motd(raw: MotdFileConfig) -> Result<MotdConfig, String> {
    Ok(MotdConfig {
        mode: match raw
            .mode
            .ok_or_else(|| "missing transport.motd.mode after defaults applied".to_string())?
        {
            MotdModeLiteral::Local => MotdMode::Local,
            MotdModeLiteral::Upstream => MotdMode::Upstream,
        },
        local_json: raw.json,
        upstream_addr: raw
            .upstream_addr
            .as_deref()
            .map(|addr| normalize_target_addr(addr, 25565))
            .transpose()?,
        protocol_mode: normalize_protocol_mode(
            raw.protocol.ok_or_else(|| {
                "missing transport.motd.protocol after defaults applied".to_string()
            })?,
        ),
        ping_mode: normalize_ping_mode(raw.ping_mode.ok_or_else(|| {
            "missing transport.motd.ping_mode after defaults applied".to_string()
        })?),
        upstream_ping_timeout: Duration::from_millis(raw.upstream_ping_timeout_ms.ok_or_else(
            || "missing transport.motd.upstream_ping_timeout_ms after defaults applied".to_string(),
        )?),
        status_cache_ttl: Duration::from_millis(raw.status_cache_ttl_ms.ok_or_else(|| {
            "missing transport.motd.status_cache_ttl_ms after defaults applied".to_string()
        })?),
        rewrite: normalize_motd_rewrite(raw.rewrite)?,
        favicon: normalize_favicon_mode(
            raw.favicon.ok_or_else(|| {
                "missing transport.motd.favicon after defaults applied".to_string()
            })?,
        )?,
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

fn normalize_favicon_mode(raw: MotdFaviconFileConfig) -> Result<MotdFaviconMode, String> {
    Ok(
        match raw.mode.ok_or_else(|| {
            "missing transport.motd.favicon.mode after defaults applied".to_string()
        })? {
            MotdFaviconModeLiteral::Passthrough => MotdFaviconMode::Passthrough,
            MotdFaviconModeLiteral::Remove => MotdFaviconMode::Remove,
            MotdFaviconModeLiteral::Override => {
                MotdFaviconMode::Override(raw.value.unwrap_or_default())
            }
        },
    )
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

fn normalize_socket_options(raw: SocketOptionsFileConfig) -> Result<SocketOptions, String> {
    Ok(SocketOptions {
        tcp_nodelay: raw.tcp_nodelay.ok_or_else(|| {
            "missing inbound.socket.tcp_nodelay after defaults applied".to_string()
        })?,
        keepalive: Some(Duration::from_secs(raw.keepalive_secs.ok_or_else(
            || "missing inbound.socket.keepalive_secs after defaults applied".to_string(),
        )?)),
        recv_buffer_size: raw.recv_buffer_size,
        send_buffer_size: raw.send_buffer_size,
        reuse_port: raw.reuse_port.ok_or_else(|| {
            "missing inbound.socket.reuse_port after defaults applied".to_string()
        })?,
    })
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
