use std::borrow::Cow;

use super::config::{RelayMode, TransportConfig};
use super::players::PlayerRegistry;

#[cfg(test)]
mod test;

pub struct TemplateContext<'a> {
    transport: &'a TransportConfig,
    relay_mode: RelayMode,
    online_players: i32,
}

impl<'a> TemplateContext<'a> {
    pub fn for_transport(
        transport: &'a TransportConfig,
        relay_mode: RelayMode,
        players: &PlayerRegistry,
    ) -> Self {
        Self {
            transport,
            relay_mode,
            online_players: players.current_online_count(),
        }
    }
}

pub fn render<'a>(value: &'a str, context: &TemplateContext<'_>) -> Cow<'a, str> {
    if !value.contains('%') {
        return Cow::Borrowed(value);
    }

    let mut rendered = value.to_owned();

    let ping_target_addr = context
        .transport
        .motd
        .ping
        .target_addr
        .clone()
        .or_else(|| context.transport.motd.upstream_addr.clone())
        .unwrap_or_default();

    let favicon_target_addr = context
        .transport
        .motd
        .favicon
        .target_addr
        .clone()
        .or_else(|| context.transport.motd.upstream_addr.clone())
        .unwrap_or_default();

    let upstream_addr = context
        .transport
        .motd
        .upstream_addr
        .clone()
        .unwrap_or_default();

    rendered = rendered.replace("%ONLINE_PLAYER%", &context.online_players.to_string());
    rendered = rendered.replace("%MOTD_TARGET_ADDR%", &upstream_addr);
    rendered = rendered.replace("%PING_TARGET_ADDR%", &ping_target_addr);
    rendered = rendered.replace("%FAVICON_TARGET_ADDR%", &favicon_target_addr);
    rendered = rendered.replace("%RELAY_MODE%", context.relay_mode.as_placeholder_value());
    rendered = rendered.replace(
        "%PING_MODE%",
        context.transport.motd.ping_mode.as_placeholder_value(),
    );
    rendered = rendered.replace(
        "%FAVICON_MODE%",
        context.transport.motd.favicon.mode.as_placeholder_value(),
    );
    rendered = rendered.replace("%UPSTREAM_ADDR%", &upstream_addr);

    Cow::Owned(rendered)
}
