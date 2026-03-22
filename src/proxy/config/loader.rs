use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::checker::ConfigChecker;
use super::normalizer::ConfigNormalizer;
use super::types::Config;

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_default() -> Result<Config, String> {
        Self::load_from_path(Path::new("config.toml"))
    }

    pub fn load_from_path(path: &Path) -> Result<Config, String> {
        let content = fs::read_to_string(path)
            .map_err(|error| format!("failed to read config {}: {error}", path.display()))?;
        let raw = toml::from_str::<RawConfig>(&content)
            .map_err(|error| format!("failed to parse TOML config {}: {error}", path.display()))?;

        let config = ConfigNormalizer::new().normalize(raw, path.to_path_buf())?;
        ConfigChecker::new().validate(&config)?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct RawConfig {
    pub inbound: RawInboundConfig,
    pub outbounds: Vec<RawOutboundRoute>,
    #[serde(default)]
    pub transport: RawTransportConfig,
    #[serde(default)]
    pub relay: RawRelayConfig,
    pub api: Option<RawApiConfig>,
    #[serde(default)]
    pub runtime: RawRuntimeConfig,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawInboundConfig {
    pub listen_addr: String,
    #[serde(default = "default_first_packet_timeout_ms")]
    pub first_packet_timeout_ms: u64,
    #[serde(default)]
    pub socket: RawSocketOptions,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawOutboundRoute {
    pub match_host: Option<String>,
    pub outbound: RawOutboundConfig,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawOutboundConfig {
    pub name: String,
    pub target_addr: String,
    pub rewrite_addr: Option<String>,
    #[serde(default)]
    pub socket: RawSocketOptions,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawTransportConfig {
    #[serde(default)]
    pub motd: RawMotdConfig,
    pub kick_json: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawRelayConfig {
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawApiConfig {
    pub base_url: String,
    pub bearer_token: Option<String>,
    pub timeout_ms: Option<u64>,
    pub traffic_interval_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawMotdConfig {
    pub mode: Option<String>,
    pub json: Option<String>,
    pub protocol: Option<String>,
    pub ping_mode: Option<String>,
    pub upstream_ping_timeout_ms: Option<u64>,
    pub status_cache_ttl_ms: Option<u64>,
    #[serde(default)]
    pub rewrite: RawMotdRewrite,
    #[serde(default)]
    pub favicon: RawMotdFavicon,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawMotdRewrite {
    pub description_pattern: Option<String>,
    pub description_replacement: Option<String>,
    pub favicon_pattern: Option<String>,
    pub favicon_replacement: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawMotdFavicon {
    pub mode: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawRuntimeConfig {
    pub stats_log_interval_secs: Option<u64>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub(super) struct RawSocketOptions {
    pub tcp_nodelay: Option<bool>,
    pub keepalive_secs: Option<u64>,
    pub recv_buffer_size: Option<usize>,
    pub send_buffer_size: Option<usize>,
    pub reuse_port: Option<bool>,
}

fn default_first_packet_timeout_ms() -> u64 {
    5_000
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::proxy::config::types::StatusPingMode;

    #[test]
    fn parse_toml_config_with_default_port() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [[outbounds]]
                [outbounds.outbound]
                name = "default"
                target_addr = "backend"
                rewrite_addr = "mc.hypixel.net"

                [runtime]
                stats_log_interval_secs = 5
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        ConfigChecker::new().validate(&config).unwrap();

        assert_eq!(config.outbounds[0].outbound.target_addr, "backend:25565");
        assert_eq!(
            config.outbounds[0].outbound.rewrite_addr,
            "mc.hypixel.net:25565"
        );
        assert_eq!(config.stats_log_interval, Some(Duration::from_secs(5)));
        assert_eq!(config.transport.motd.ping_mode, StatusPingMode::Passthrough);
    }

    #[test]
    fn route_hosts_are_normalized() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [[outbounds]]
                match_host = "MC.HYPIXEL.NET."
                [outbounds.outbound]
                name = "hypixel"
                target_addr = "srv-backend:25570"
                rewrite_addr = "mc.hypixel.net"

                [[outbounds]]
                [outbounds.outbound]
                name = "fallback"
                target_addr = "fallback:25565"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        ConfigChecker::new().validate(&config).unwrap();

        assert_eq!(
            config.outbounds[0].match_host.as_deref(),
            Some("mc.hypixel.net")
        );
        assert_eq!(
            config.outbounds[0].outbound.target_addr,
            "srv-backend:25570"
        );
    }

    #[test]
    fn parses_zero_ping_mode() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [[outbounds]]
                [outbounds.outbound]
                name = "default"
                target_addr = "backend:25565"
                rewrite_addr = "example.com"

                [transport.motd]
                ping_mode = "0ms"
                upstream_ping_timeout_ms = 2500
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        ConfigChecker::new().validate(&config).unwrap();

        assert_eq!(config.transport.motd.ping_mode, StatusPingMode::ZeroMs);
        assert_eq!(
            config.transport.motd.upstream_ping_timeout,
            Duration::from_millis(2500)
        );
    }

    #[test]
    fn rewrite_addr_defaults_to_target_addr() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [[outbounds]]
                [outbounds.outbound]
                name = "default"
                target_addr = "backend"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        ConfigChecker::new().validate(&config).unwrap();

        assert_eq!(config.outbounds[0].outbound.target_addr, "backend:25565");
        assert_eq!(config.outbounds[0].outbound.rewrite_addr, "backend:25565");
    }

    #[test]
    fn requires_single_fallback_outbound() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [[outbounds]]
                match_host = "example.com"
                [outbounds.outbound]
                name = "only"
                target_addr = "backend:25565"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        let error = ConfigChecker::new().validate(&config).unwrap_err();
        assert!(error.contains("fallback"));
    }
}
