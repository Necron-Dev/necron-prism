#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::proxy::config::checker::ConfigChecker;
    use crate::proxy::config::normalizer::ConfigNormalizer;
    use crate::proxy::config::schema_types::{ConfigFile, MotdFaviconModeLiteral};
    use crate::proxy::config::{ApiMode, ConfigLoader, MotdFaviconMode};

    #[test]
    fn parse_mock_api_config() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                connection_id_prefix = "mock"

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
        assert_eq!(config.api.mock.rewrite_addr, "backend:25565");
        assert_eq!(config.stats_log_interval, Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_mock_api_config_defaults_rewrite_addr_to_target() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                connection_id_prefix = "mock"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();

        assert_eq!(config.api.mock.target_addr, "backend:25565");
        assert_eq!(config.api.mock.rewrite_addr, "backend:25565");
    }

    #[test]
    fn parse_mock_api_config_normalizes_explicit_rewrite_addr() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                rewrite_addr = "rewrite-host"
                connection_id_prefix = "mock"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();

        assert_eq!(config.api.mock.target_addr, "backend:25565");
        assert_eq!(config.api.mock.rewrite_addr, "rewrite-host:25565");
    }

    #[test]
    fn parse_upstream_motd_addr() {
        let raw = toml::from_str::<ConfigFile>(
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
                connection_id_prefix = "mock"
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
    fn parse_upstream_tcp_ping_mode() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [transport.motd]
                mode = "local"
                ping_mode = "upstream_tcp"
                upstream_addr = "status-backend"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                connection_id_prefix = "mock"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();

        assert_eq!(
            config.transport.motd.mode,
            crate::proxy::config::MotdMode::Local
        );
        assert_eq!(
            config.transport.motd.ping_mode,
            crate::proxy::config::StatusPingMode::UpstreamTcp
        );
        assert_eq!(
            config.transport.motd.upstream_addr.as_deref(),
            Some("status-backend:25565")
        );
        assert_eq!(config.transport.motd.ping.target_addr.as_deref(), None);
    }

    #[test]
    fn local_passthrough_favicon_uses_explicit_target_addr() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [transport.motd]
                mode = "local"
                upstream_addr = "status-backend"

                [transport.motd.favicon]
                mode = "passthrough"
                target_addr = "icon-backend"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                connection_id_prefix = "mock"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();

        assert_eq!(
            config.transport.motd.upstream_addr.as_deref(),
            Some("status-backend:25565")
        );
        assert!(matches!(
            config.transport.motd.favicon.mode,
            MotdFaviconMode::Passthrough
        ));
        assert_eq!(
            config.transport.motd.favicon.target_addr.as_deref(),
            Some("icon-backend:25565")
        );
    }

    #[test]
    fn parse_ping_target_addr_override() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [transport.motd]
                mode = "local"
                ping_mode = "upstream_tcp"

                [transport.motd.ping]
                target_addr = "ping-backend"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                connection_id_prefix = "mock"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();

        assert_eq!(
            config.transport.motd.ping.target_addr.as_deref(),
            Some("ping-backend:25565")
        );
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
        assert!(written.contains("mode = \"json\""));
        assert!(written.contains("[transport.motd.ping]"));
        assert_eq!(config.inbound.listen_addr, "0.0.0.0:25565");
        assert!(matches!(
            config.transport.motd.favicon.mode,
            MotdFaviconMode::Json
        ));

        let _ = fs::remove_file(config_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn default_motd_favicon_mode_is_json() {
        assert!(matches!(
            MotdFaviconModeLiteral::default(),
            MotdFaviconModeLiteral::Json
        ));
    }

    #[test]
    fn checker_requires_favicon_path_for_path_mode() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [transport.motd.favicon]
                mode = "path"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                connection_id_prefix = "mock"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("config.toml"))
            .unwrap();
        let error = ConfigChecker::new().validate(&config).unwrap_err();

        assert!(error.to_string().contains("favicon.path"));
    }

    #[test]
    fn normalizes_favicon_path_relative_to_config() {
        let raw = toml::from_str::<ConfigFile>(
            r#"
                [inbound]
                listen_addr = "0.0.0.0:25565"

                [transport.motd.favicon]
                mode = "path"
                path = "assets/icon.png"

                [api]
                mode = "mock"

                [api.mock]
                target_addr = "backend"
                connection_id_prefix = "mock"
            "#,
        )
        .unwrap();

        let config = ConfigNormalizer::new()
            .normalize(raw, PathBuf::from("nested/config.toml"))
            .unwrap();

        assert_eq!(
            config.transport.motd.favicon.path.as_deref(),
            Some(Path::new("nested").join("assets/icon.png").as_path())
        );
    }

    #[test]
    fn loader_requires_http_base_url() {
        let raw = toml::from_str::<ConfigFile>(
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
        assert!(error.to_string().contains("api.base_url"));
    }
}
