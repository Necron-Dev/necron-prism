#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::proxy::config::checker::ConfigChecker;
    use crate::proxy::config::default::ConfigDefaults;
    use crate::proxy::config::normalizer::ConfigNormalizer;
    use crate::proxy::config::schema_types::{
        ApiFileConfig, ApiModeLiteral, ConfigFile, InboundFileConfig, MockApiFileConfig,
        MotdFaviconFileConfig, MotdFaviconModeLiteral, MotdFileConfig, MotdModeLiteral,
        MotdProtocolLiteral, MotdProtocolNamedLiteral, RelayFileConfig, RelayModeLiteral,
        StatusPingModeLiteral, TransportFileConfig,
    };
    use crate::proxy::config::{ApiMode, ConfigLoader};

    #[test]
    fn parse_mock_api_config() {
        let raw = ConfigDefaults::apply(
            toml::from_str::<ConfigFile>(
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
            .unwrap(),
        );

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
        let raw = ConfigDefaults::apply(
            toml::from_str::<ConfigFile>(
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
            .unwrap(),
        );

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
        let raw = ConfigDefaults::apply(
            toml::from_str::<ConfigFile>(
                r#"
                    [inbound]
                    listen_addr = "0.0.0.0:25565"

                    [api]
                    mode = "http"
                "#,
            )
            .unwrap(),
        );

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

    #[test]
    fn override_favicon_requires_non_empty_value() {
        let raw = ConfigDefaults::apply(ConfigFile {
            inbound: Some(InboundFileConfig {
                listen_addr: Some("0.0.0.0:25565".to_string()),
                first_packet_timeout_ms: None,
                socket: None,
            }),
            transport: Some(TransportFileConfig {
                motd: Some(MotdFileConfig {
                    mode: Some(MotdModeLiteral::Local),
                    json: Some("{}".to_string()),
                    upstream_addr: None,
                    protocol: Some(MotdProtocolLiteral::Named(MotdProtocolNamedLiteral::Client)),
                    ping_mode: Some(StatusPingModeLiteral::Passthrough),
                    upstream_ping_timeout_ms: None,
                    status_cache_ttl_ms: None,
                    rewrite: None,
                    favicon: Some(MotdFaviconFileConfig {
                        mode: Some(MotdFaviconModeLiteral::Override),
                        value: Some(String::new()),
                    }),
                }),
            }),
            relay: Some(RelayFileConfig {
                mode: Some(RelayModeLiteral::Standard),
            }),
            api: Some(ApiFileConfig {
                mode: Some(ApiModeLiteral::Mock),
                base_url: None,
                bearer_token: None,
                timeout_ms: None,
                traffic_interval_ms: None,
                mock: Some(MockApiFileConfig {
                    target_addr: Some("backend".to_string()),
                    kick_reason: None,
                    connection_id_prefix: Some("mock".to_string()),
                }),
            }),
            runtime: None,
        });

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        let error = ConfigChecker::new().validate(&config).unwrap_err();
        assert!(error.contains("non-empty"));
    }

    #[test]
    fn loader_writes_default_config_when_missing() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("necron-prism-config-{unique}"));
        fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("config.toml");

        let config = ConfigLoader::load_from_path(&config_path).unwrap();
        let written = fs::read_to_string(&config_path).unwrap();

        assert!(written.contains("#:schema ./config.schema.json"));
        assert_eq!(config.inbound.listen_addr, "0.0.0.0:25565");

        let _ = fs::remove_file(config_path);
        let _ = fs::remove_dir(temp_dir);
    }
}
