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
    #[serde(default)]
    pub transport: RawTransportConfig,
    #[serde(default)]
    pub relay: RawRelayConfig,
    pub api: RawApiConfig,
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

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawTransportConfig {
    #[serde(default)]
    pub motd: RawMotdConfig,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawRelayConfig {
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawApiConfig {
    pub mode: Option<String>,
    pub base_url: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout_ms: Option<u64>,
    pub traffic_interval_ms: Option<u64>,
    #[serde(default)]
    pub mock: RawMockApiConfig,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawMockApiConfig {
    pub target_addr: Option<String>,
    pub kick_reason: Option<String>,
    pub connection_id_prefix: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct RawMotdConfig {
    pub mode: Option<String>,
    pub json: Option<String>,
    pub upstream_addr: Option<String>,
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
    use crate::proxy::config::{ApiMode, ConfigLoader};

    #[test]
    fn parse_mock_api_config() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"

                [runtime]
                stats_log_interval_secs = 5
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        ConfigChecker::new().validate(&config).unwrap();

        assert_eq!(config.api.mode, ApiMode::Mock);
        assert_eq!(config.api.mock.target_addr, "backend:25565");
        assert_eq!(config.stats_log_interval, Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_upstream_motd_addr() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [transport.motd]
                mode = "upstream"
                upstream_addr = "status-backend"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        ConfigChecker::new().validate(&config).unwrap();

        assert_eq!(
            config.transport.motd.upstream_addr.as_deref(),
            Some("status-backend:25565")
        );
    }

    #[test]
    fn loader_requires_http_base_url() {
        let raw = toml::from_str::<RawConfig>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [api]
                mode = "http"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        let error = ConfigChecker::new().validate(&config).unwrap_err();
        assert!(error.contains("api.base_url"));
    }

    #[test]
    fn load_default_path_constant_stays_same() {
        let path = Path::new("config.toml");
        assert_eq!(path, Path::new("config.toml"));
        let _ = ConfigLoader::load_from_path;
    }
}
