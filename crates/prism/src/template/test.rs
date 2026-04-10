use super::*;
use crate::config::{
    MotdConfig, MotdFaviconConfig, MotdFaviconMode, MotdMode, MotdProtocol, RelayConfig, RelayMode,
    StatusPingMode,
};
use serde_json::Value;

#[test]
fn render_replaces_all_supported_placeholders() {
    // Template tests don't need ConnectionRegistry anymore
    let config = MotdConfig {
        mode: MotdMode::Local,
        local_json: "{online_player}|{motd_target_addr}|{ping_target_addr}|{favicon_target_addr}|{relay_mode}|{ping_mode}|{favicon_mode}|{upstream_addr}".to_owned(),
        upstream_addr: "motd.example:25565".to_owned(),
        protocol: MotdProtocol::Client,
        ping_mode: StatusPingMode::Passthrough,
        ping_target_addr: Some("ping.example:25565".to_owned()),
        upstream_ping_timeout_ms: 1000,
        favicon: MotdFaviconConfig {
            mode: MotdFaviconMode::Passthrough,
            path: None,
            target_addr: Some("icon.example:25565".to_owned()),
        },
    };
    let relay = RelayConfig {
        mode: RelayMode::Async,
    };
    let context = TemplateContext::for_transport(&config, &relay, 1);

    let rendered = render(&config.local_json, &context);

    assert_eq!(
        rendered,
        "1|motd.example:25565|ping.example:25565|icon.example:25565|async|passthrough|passthrough|motd.example:25565"
    );
}

#[test]
fn render_uses_relay_label_and_target_fallbacks() {
    let config = MotdConfig {
        mode: MotdMode::Local,
        local_json: "{motd_target_addr}|{ping_target_addr}|{favicon_target_addr}|{relay_mode}"
            .to_owned(),
        upstream_addr: "motd.example:25565".to_owned(),
        protocol: MotdProtocol::Client,
        ping_mode: StatusPingMode::Local,
        ping_target_addr: None,
        upstream_ping_timeout_ms: 1000,
        favicon: MotdFaviconConfig {
            mode: MotdFaviconMode::Json,
            path: None,
            target_addr: None,
        },
    };
    let relay = RelayConfig {
        mode: RelayMode::IoUring,
    };

    let rendered = render(
        &config.local_json,
        &TemplateContext::for_transport(&config, &relay, 0),
    );

    assert_eq!(
        rendered,
        "motd.example:25565|motd.example:25565|motd.example:25565|io_uring"
    );
}

#[test]
fn render_uses_upstream_fallback_when_ping_and_favicon_targets_are_missing() {
    let config = MotdConfig {
        mode: MotdMode::Local,
        local_json: "{ping_target_addr}|{favicon_target_addr}".to_owned(),
        upstream_addr: "motd.example:25565".to_owned(),
        protocol: MotdProtocol::Client,
        ping_mode: StatusPingMode::Local,
        ping_target_addr: None,
        upstream_ping_timeout_ms: 1000,
        favicon: MotdFaviconConfig {
            mode: MotdFaviconMode::Json,
            path: None,
            target_addr: None,
        },
    };
    let relay = RelayConfig {
        mode: RelayMode::Async,
    };

    let rendered = render(
        &config.local_json,
        &TemplateContext::for_transport(&config, &relay, 0),
    );

    assert_eq!(rendered, "motd.example:25565|motd.example:25565");
}

#[test]
fn render_keeps_explicit_favicon_target_even_when_ping_falls_back() {
    let config = MotdConfig {
        mode: MotdMode::Local,
        local_json: "{ping_target_addr}|{favicon_target_addr}".to_owned(),
        upstream_addr: "motd.example:25565".to_owned(),
        protocol: MotdProtocol::Client,
        ping_mode: StatusPingMode::Local,
        ping_target_addr: None,
        upstream_ping_timeout_ms: 1000,
        favicon: MotdFaviconConfig {
            mode: MotdFaviconMode::Json,
            path: None,
            target_addr: Some("icon.example:25565".to_owned()),
        },
    };
    let relay = RelayConfig {
        mode: RelayMode::Async,
    };

    let rendered = render(
        &config.local_json,
        &TemplateContext::for_transport(&config, &relay, 0),
    );

    assert_eq!(rendered, "motd.example:25565|icon.example:25565");
}

#[test]
fn render_default_local_json_stays_valid_json() {
    let config = MotdConfig::default();
    let relay = RelayConfig {
        mode: RelayMode::Async,
    };

    let rendered = render(
        &config.local_json,
        &TemplateContext::for_transport(&config, &relay, 1),
    );

    let json: Value = serde_json::from_str(&rendered).expect("rendered MOTD JSON should parse");
    assert_eq!(json["players"]["online"], 1);
    assert_eq!(json["players"]["max"], 100);
}
