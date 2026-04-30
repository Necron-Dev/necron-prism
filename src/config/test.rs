use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use prism::config::*;

use crate::config::{ApiMode, ConfigLoader, canonicalize_runtime_config};

#[test]
fn parse_minimal_config() {
    let mut config = ConfigLoader::load_from_str(
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
    canonicalize_runtime_config(&mut config);

    assert_eq!(config.api.mode, ApiMode::Mock);
    assert_eq!(config.prism.network.socket.listen_addr, "127.0.0.1:25565");
    assert_eq!(config.prism.network.relay.mode, RelayMode::Async);
}

#[cfg_attr(miri, ignore)]
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
    assert!(!written.contains("[relay]"));
    assert!(written.contains("[network.socket]"));
    assert!(written.contains("[logging]"));
    assert!(written.contains("level = \"info\""));
    assert!(written.contains("stats_log_interval_secs = 10"));
    assert_eq!(config.prism.network.socket.listen_addr, "0.0.0.0:25565");
}

#[test]
fn validate_requires_http_base_url() {
    let result = ConfigLoader::load_from_str(
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [api]
            mode = "http"

            [network.relay]
        "#,
    );
    assert!(result.is_err());
}

#[test]
fn parse_explicit_relay_config() {
    let mut config = ConfigLoader::load_from_str(
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [network.relay]
            mode = "splice"

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();
    canonicalize_runtime_config(&mut config);

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    assert_eq!(config.prism.network.relay.mode, RelayMode::Splice);
    #[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
    {
        assert_eq!(config.prism.network.relay.mode, RelayMode::Async);
        assert_eq!(config.prism.requested_relay.mode, RelayMode::Splice);
    }
}

#[test]
fn parse_io_uring_config() {
    let mut config = ConfigLoader::load_from_str(
        r#"
            [network.socket]
            listen_addr = "127.0.0.1:25565"

            [network.relay]
            mode = "io_uring"

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();
    canonicalize_runtime_config(&mut config);

    #[cfg(all(target_os = "linux", feature = "linux-accel"))]
    assert_eq!(config.prism.network.relay.mode, RelayMode::IoUring);
    #[cfg(not(all(target_os = "linux", feature = "linux-accel")))]
    {
        assert_eq!(config.prism.network.relay.mode, RelayMode::Async);
        assert_eq!(config.prism.requested_relay.mode, RelayMode::IoUring);
    }
}

#[test]
fn parse_empty_relay_section_uses_defaults() {
    let config = ConfigLoader::load_from_str(
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

    assert_eq!(config.prism.network.relay.mode, RelayMode::Async);
}

#[test]
fn parse_logging_stats_interval_config() {
    let config = ConfigLoader::load_from_str(
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

    assert_eq!(config.prism.logging.stats_log_interval_secs, Some(42));
}

#[test]
fn canonicalize_updates_requested_relay_on_non_linux() {
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
            mode = "io_uring"

            [api]
            mode = "mock"
            mock_target_addr = "127.0.0.1"
        "#,
    )
    .unwrap();

    let mut config = ConfigLoader::load_from_path(&config_path).unwrap();
    canonicalize_runtime_config(&mut config);

    assert_eq!(config.prism.network.relay.mode, RelayMode::Async);
    assert_eq!(config.prism.network.relay.label(), "async");
    assert_eq!(config.prism.requested_relay.mode, RelayMode::IoUring);

    let _ = fs::remove_file(config_path);
    let _ = fs::remove_dir(temp_dir);
}
