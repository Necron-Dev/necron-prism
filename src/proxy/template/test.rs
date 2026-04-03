use std::time::Duration;

use super::*;
use crate::proxy::config::{
    MotdConfig, MotdFaviconConfig, MotdFaviconMode, MotdMode, MotdPingConfig, MotdProtocolMode,
    StatusPingMode, TransportConfig,
};
use crate::proxy::players::PlayerRegistry;

#[test]
fn render_replaces_all_supported_placeholders() {
    let players = PlayerRegistry::default();
    players.register_connection(1);
    players.register_connection(2);
    players.update_outbound(1, "alpha:25565".into());

    let transport = TransportConfig {
        motd: MotdConfig {
            mode: MotdMode::Local,
            local_json: "%ONLINE_PLAYER%|%MOTD_TARGET_ADDR%|%PING_TARGET_ADDR%|%FAVICON_TARGET_ADDR%|%RELAY_MODE%|%PING_MODE%|%FAVICON_MODE%|%UPSTREAM_ADDR%".to_owned(),
            upstream_addr: Some("motd.example:25565".to_owned()),
            protocol_mode: MotdProtocolMode::Client,
            ping_mode: StatusPingMode::Passthrough,
            ping: MotdPingConfig {
                target_addr: Some("ping.example:25565".to_owned()),
            },
            upstream_ping_timeout: Duration::from_secs(1),
            status_cache_ttl: Duration::from_secs(1),
            favicon: MotdFaviconConfig {
                mode: MotdFaviconMode::Passthrough,
                path: None,
                target_addr: Some("icon.example:25565".to_owned()),
            },
        },
    };
    let context = TemplateContext::for_transport(&transport, RelayMode::Standard, &players);

    let rendered = render(&transport.motd.local_json, &context);

    assert_eq!(
        rendered,
        "1|motd.example:25565|ping.example:25565|icon.example:25565|standard|passthrough|passthrough|motd.example:25565"
    );
}
