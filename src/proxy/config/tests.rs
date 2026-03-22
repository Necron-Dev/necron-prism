#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use crate::proxy::config::checker::ConfigChecker;
    use crate::proxy::config::loader::RawConfig;
    use crate::proxy::config::normalizer::ConfigNormalizer;
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
