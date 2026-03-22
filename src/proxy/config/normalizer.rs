use std::net::Ipv6Addr;
use std::path::PathBuf;
use std::time::Duration;

use regex::Regex;

use super::loader::{
    RawConfig, RawMotdConfig, RawMotdFavicon, RawMotdRewrite, RawOutboundConfig, RawOutboundRoute,
    RawRelayConfig, RawSocketOptions,
};
use super::types::{
    Config, InboundConfig, MotdConfig, MotdFaviconMode, MotdMode, MotdProtocolMode, MotdRewrite,
    OutboundConfig, OutboundRoute, RelayConfig, RelayMode, SocketOptions, StatusPingMode,
    TransportConfig,
};

pub struct ConfigNormalizer;

impl ConfigNormalizer {
    pub fn new() -> Self {
        Self
    }

    pub fn normalize(&self, raw: RawConfig, source_path: PathBuf) -> Result<Config, String> {
        let outbounds = raw
            .outbounds
            .into_iter()
            .map(normalize_outbound_route)
            .collect::<Result<Vec<_>, String>>()?;

        Ok(Config {
            inbound: InboundConfig {
                listen_addr: raw.inbound.listen_addr,
                first_packet_timeout: Duration::from_millis(raw.inbound.first_packet_timeout_ms),
                socket_options: normalize_socket_options(raw.inbound.socket),
            },
            outbounds,
            transport: TransportConfig {
                motd: normalize_motd(raw.transport.motd)?,
                kick_json: raw.transport.kick_json,
            },
            relay: normalize_relay(raw.relay)?,
            stats_log_interval: raw
                .runtime
                .stats_log_interval_secs
                .map(Duration::from_secs)
                .or(Some(Duration::from_secs(10))),
            source_path,
        })
    }
}

fn normalize_relay(raw: RawRelayConfig) -> Result<RelayConfig, String> {
    Ok(RelayConfig {
        mode: match raw.mode.as_deref().unwrap_or("standard") {
            "standard" | "copy" => RelayMode::Standard,
            "splice" | "linux_splice" | "linux-splice" => RelayMode::LinuxSplice,
            other => return Err(format!("invalid relay.mode: {other}")),
        },
    })
}

fn normalize_outbound_route(raw: RawOutboundRoute) -> Result<OutboundRoute, String> {
    Ok(OutboundRoute {
        match_host: raw.match_host.as_deref().map(normalize_host),
        outbound: normalize_outbound(raw.outbound)?,
    })
}

fn normalize_outbound(raw: RawOutboundConfig) -> Result<OutboundConfig, String> {
    let target_addr = normalize_target_addr(&raw.target_addr, 25565)?;
    let rewrite_addr = match raw.rewrite_addr {
        Some(addr) => normalize_target_addr(&addr, parse_target_port(&target_addr, 25565)?)?,
        None => target_addr.clone(),
    };

    Ok(OutboundConfig {
        name: raw.name,
        target_addr,
        rewrite_addr,
        socket_options: normalize_socket_options(raw.socket),
    })
}

fn normalize_motd(raw: RawMotdConfig) -> Result<MotdConfig, String> {
    Ok(MotdConfig {
        mode: normalize_motd_mode(raw.mode.as_deref())?,
        local_json: raw.json,
        protocol_mode: normalize_protocol_mode(raw.protocol.as_deref())?,
        ping_mode: normalize_ping_mode(raw.ping_mode.as_deref())?,
        upstream_ping_timeout: Duration::from_millis(raw.upstream_ping_timeout_ms.unwrap_or(1500)),
        status_cache_ttl: Duration::from_millis(raw.status_cache_ttl_ms.unwrap_or(1000)),
        rewrite: normalize_motd_rewrite(raw.rewrite)?,
        favicon: normalize_favicon_mode(raw.favicon)?,
    })
}

fn normalize_motd_rewrite(raw: RawMotdRewrite) -> Result<Option<MotdRewrite>, String> {
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

fn normalize_favicon_mode(raw: RawMotdFavicon) -> Result<MotdFaviconMode, String> {
    match raw.mode.as_deref().unwrap_or("passthrough") {
        "passthrough" => Ok(MotdFaviconMode::Passthrough),
        "remove" => Ok(MotdFaviconMode::Remove),
        "override" => raw.value.map(MotdFaviconMode::Override).ok_or_else(|| {
            "transport.motd.favicon.mode=override requires favicon.value".to_string()
        }),
        other => Err(format!("invalid transport.motd.favicon.mode: {other}")),
    }
}

fn normalize_motd_mode(value: Option<&str>) -> Result<MotdMode, String> {
    match value.unwrap_or("local") {
        "local" => Ok(MotdMode::Local),
        "upstream" => Ok(MotdMode::Upstream),
        other => Err(format!("invalid transport.motd.mode: {other}")),
    }
}

fn normalize_protocol_mode(value: Option<&str>) -> Result<MotdProtocolMode, String> {
    match value.unwrap_or("client") {
        "client" => Ok(MotdProtocolMode::Client),
        "-1" | "negative_one" => Ok(MotdProtocolMode::NegativeOne),
        other => other
            .parse::<i32>()
            .map(MotdProtocolMode::Fixed)
            .map_err(|_| format!("invalid transport.motd.protocol: {other}")),
    }
}

fn normalize_socket_options(raw: RawSocketOptions) -> SocketOptions {
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

fn normalize_ping_mode(value: Option<&str>) -> Result<StatusPingMode, String> {
    match value.unwrap_or("passthrough") {
        "passthrough" | "echo" => Ok(StatusPingMode::Passthrough),
        "0ms" | "zero" | "zero_ms" | "zeroping" => Ok(StatusPingMode::ZeroMs),
        "upstream_tcp" | "upstream-tcp" | "tcp" => Ok(StatusPingMode::UpstreamTcp),
        "disconnect" => Ok(StatusPingMode::Disconnect),
        other => Err(format!("invalid transport.motd.ping_mode: {other}")),
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

fn normalize_host(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}
