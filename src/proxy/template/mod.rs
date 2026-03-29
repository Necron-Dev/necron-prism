use minijinja::context;
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

    let mut env = minijinja::Environment::new();
    env.add_template("tpl", value).unwrap();
    let tpl = env.get_template("tpl").unwrap();

    let rendered = tpl
        .render(context! {
            ONLINE_PLAYER => context.online_players,
            MOTD_TARGET_ADDR => context.transport.motd.upstream_addr.clone().unwrap_or_default(),
            PING_TARGET_ADDR => ping_target_addr,
            FAVICON_TARGET_ADDR => favicon_target_addr,
            RELAY_MODE => context.relay_mode.as_placeholder_value(),
            PING_MODE => context.transport.motd.ping_mode.as_placeholder_value(),
            FAVICON_MODE => context.transport.motd.favicon.mode.as_placeholder_value(),
            UPSTREAM_ADDR => context.transport.motd.upstream_addr.clone().unwrap_or_default(),
        })
        .unwrap();

    Cow::Owned(rendered)
}
