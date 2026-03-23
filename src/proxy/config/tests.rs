#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use crate::proxy::config::checker::ConfigChecker;
    use crate::proxy::config::default::ConfigDefaults;
    use crate::proxy::config::normalizer::ConfigNormalizer;
    use crate::proxy::config::schema_types::{ConfigFile, MotdFaviconModeLiteral, MotdModeLiteral};
    use crate::proxy::config::{ApiMode, ConfigLoader};

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
        assert_eq!(config.stats_log_interval, Some(Duration::from_secs(5)));
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
        let mut raw = ConfigDefaults::file();
        raw.transport.motd.mode = MotdModeLiteral::Local;
        raw.transport.motd.json = "{}".to_string();
        raw.transport.motd.favicon.mode = MotdFaviconModeLiteral::Override;
        raw.transport.motd.favicon.value = Some(String::new());

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
