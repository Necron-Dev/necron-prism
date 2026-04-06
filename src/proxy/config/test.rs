use crate::proxy::config::{ApiMode, ConfigLoader, RelayConfig, RelayDataMode};
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
    assert_eq!(config.relay.mode, RelayDataMode::Async);
    assert!(!config.relay.io_uring);
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
    assert!(written.contains("[relay]"));
    assert!(written.contains("mode = \"async\""));
    assert!(written.contains("io_uring = false"));
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

#[test]
fn parse_explicit_relay_config() {
    let config = toml::from_str::<crate::proxy::config::Config>(
        r#"
            listen_addr = "127.0.0.1:25565"
            [relay]
            mode = "splice"
            io_uring = true

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.relay.mode, RelayDataMode::Splice);
    assert!(config.relay.io_uring);
}

#[test]
fn relay_label_matrix() {
    let cases = [
        (RelayDataMode::Async, false, "async"),
        (RelayDataMode::Async, true, "async+io_uring"),
        (RelayDataMode::Splice, false, "splice"),
        (RelayDataMode::Splice, true, "splice+io_uring"),
    ];

    for (mode, io_uring, expected) in cases {
        let relay = RelayConfig { mode, io_uring };
        assert_eq!(relay.label(), expected);
    }
}

#[test]
fn parse_empty_relay_section_uses_defaults() {
    let config = toml::from_str::<crate::proxy::config::Config>(
        r#"
            listen_addr = "127.0.0.1:25565"
            [relay]

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.relay.mode, RelayDataMode::Async);
    assert!(!config.relay.io_uring);
}

#[test]
fn parse_async_io_uring_config() {
    let config = toml::from_str::<crate::proxy::config::Config>(
        r#"
            listen_addr = "127.0.0.1:25565"
            [relay]
            mode = "async"
            io_uring = true

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.relay.mode, RelayDataMode::Async);
    assert!(config.relay.io_uring);
}
