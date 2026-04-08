use crate::proxy::config::{ApiMode, ConfigLoader, RelayConfig, RelayDataMode};
use crate::proxy::stats::ConnectionSession;
use std::fs;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn parse_minimal_config() {
    let config = toml::from_str::<crate::proxy::config::Config>(
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [network.relay]

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.api.mode, ApiMode::Mock);
    assert_eq!(config.network.socket.listen_addr, "127.0.0.1:25565");
    assert_eq!(config.network.relay.mode, RelayDataMode::Async);
    assert!(!config.network.relay.io_uring);
}

#[test]
fn loader_writes_default_config_when_missing() {
    struct TempConfigDir {
        path: std::path::PathBuf,
    }

    impl TempConfigDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("necron-prism-config-{unique}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn config_path(&self) -> std::path::PathBuf {
            self.path.join("config.toml")
        }
    }

    impl Drop for TempConfigDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    let temp_dir = TempConfigDir::new();
    let config_path = temp_dir.config_path();

    let config = ConfigLoader::load_from_path(&config_path).expect("failed to load");
    let written = fs::read_to_string(&config_path).unwrap();

    assert!(written.contains("#:schema ./config.schema.json"));
    assert!(written.contains("[network.relay]"));
    assert!(written.contains("mode = \"async\""));
    assert!(written.contains("io_uring = false"));
    assert!(!written.contains("listen_addr = \"0.0.0.0:25565\"\nmode = \"async\""));
    assert!(!written.contains("[relay]"));
    assert!(written.contains("[network.socket]"));
    assert!(written.contains("[logging]"));
    assert!(written.contains("level = \"info\""));
    assert!(written.contains("stats_log_interval_secs = 10"));
    assert_eq!(config.network.socket.listen_addr, "0.0.0.0:25565");
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
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [network.relay]
            mode = "splice"
            io_uring = true

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.network.relay.mode, RelayDataMode::Splice);
    assert!(config.network.relay.io_uring);
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

#[cfg(not(target_os = "linux"))]
#[test]
fn loader_canonicalizes_linux_only_relay_acceleration() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("necron-prism-config-canonical-{unique}"));
    fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [network.relay]
            mode = "splice"
            io_uring = true

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    let config = ConfigLoader::load_from_path(&config_path).unwrap();

    assert_eq!(config.network.relay.mode, RelayDataMode::Async);
    assert!(!config.network.relay.io_uring);
    assert_eq!(config.network.relay.label(), "async");
    assert_eq!(config.requested_relay.mode, RelayDataMode::Splice);
    assert!(config.requested_relay.io_uring);

    let _ = fs::remove_file(config_path);
    let _ = fs::remove_dir(temp_dir);
}

#[test]
fn parse_empty_relay_section_uses_defaults() {
    let config = toml::from_str::<crate::proxy::config::Config>(
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [network.relay]

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.network.relay.mode, RelayDataMode::Async);
    assert!(!config.network.relay.io_uring);
}

#[test]
fn parse_async_io_uring_config() {
    let config = toml::from_str::<crate::proxy::config::Config>(
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [network.relay]
            mode = "async"
            io_uring = true

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.network.relay.mode, RelayDataMode::Async);
    assert!(config.network.relay.io_uring);
}

#[test]
fn parse_logging_stats_interval_config() {
    let config = toml::from_str::<crate::proxy::config::Config>(
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [logging]
            stats_log_interval_secs = 42

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    assert_eq!(config.logging.stats_log_interval_secs, Some(42));
    assert_eq!(
        config
            .logging
            .stats_log_interval_secs
            .map(std::time::Duration::from_secs),
        Some(std::time::Duration::from_secs(42))
    );
}

#[test]
fn connection_session_keeps_identity_fields() {
    let peer_addr: SocketAddr = "127.0.0.1:25565".parse().unwrap();
    let context = ConnectionSession::new(42, Some(peer_addr));

    assert_eq!(context.id, 42);
    assert_eq!(context.peer_addr, Some(peer_addr));

    context.record_player_name("alex");
    context.record_stage("login");
    let _entered = context.enter_stage("relay");
}
