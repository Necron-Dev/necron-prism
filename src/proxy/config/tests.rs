#[cfg(test)]
mod test_cases {
    use crate::proxy::config::{ApiMode, ConfigLoader};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_minimal_config() {
        let config = toml::from_str::<crate::proxy::config::Config>(
            r#"
                listen_addr = "127.0.0.1:25565"
                [api]
                mode = "mock"
                mock_target_addr = "127.0.0.1"
            "#,
        )
        .unwrap();

        assert_eq!(config.api.mode, ApiMode::Mock);
        assert_eq!(config.listen_addr, "127.0.0.1:25565");
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

        let config = ConfigLoader::load_from_path(&config_path).expect("failed to load");
        let written = fs::read_to_string(&config_path).unwrap();

        assert!(written.contains("#:schema ./config.schema.json"));
        assert!(written.contains("[logging]"));
        assert!(written.contains("level = \"info\""));
        assert_eq!(config.listen_addr, "0.0.0.0:25565");

        let _ = fs::remove_file(config_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn validate_requires_http_base_url() {
        let config = crate::proxy::config::Config {
            api: crate::proxy::config::ApiConfig {
                mode: ApiMode::Http,
                base_url: None,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }
}
