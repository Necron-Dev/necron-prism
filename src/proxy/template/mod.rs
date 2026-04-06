use super::config::{MotdConfig, RelayConfig};
use super::players::PlayerRegistry;
use serde::Serialize;
use std::borrow::Cow;
use tinytemplate::TinyTemplate;

#[cfg(test)]
mod test;

#[derive(Serialize)]
pub struct TemplateContext {
    pub online_player: String,
    pub motd_target_addr: String,
    pub ping_target_addr: String,
    pub favicon_target_addr: String,
    pub relay_mode: String,
    pub ping_mode: String,
    pub favicon_mode: String,
    pub upstream_addr: String,
}

impl TemplateContext {
    pub fn for_transport(
        transport: &MotdConfig,
        relay: &RelayConfig,
        players: &PlayerRegistry,
    ) -> Self {
        let upstream_addr = transport.upstream_addr.clone();
        let ping_target_addr = transport
            .ping_target_addr
            .as_deref()
            .unwrap_or(&upstream_addr)
            .to_string();
        let favicon_target_addr = transport
            .favicon
            .target_addr
            .as_deref()
            .unwrap_or(&upstream_addr)
            .to_string();

        Self {
            online_player: players.current_online_count().to_string(),
            motd_target_addr: upstream_addr.clone(),
            ping_target_addr,
            favicon_target_addr,
            relay_mode: relay.label().to_string(),
            ping_mode: transport.ping_mode.to_string(),
            favicon_mode: transport.favicon.mode.to_string(),
            upstream_addr,
        }
    }
}

pub fn render<'a>(template_str: &'a str, context: &TemplateContext) -> Cow<'a, str> {
    if !template_str.contains('{') {
        return Cow::Borrowed(template_str);
    }

    let mut tt = TinyTemplate::new();
    if tt.add_template("t", template_str).is_err() {
        return Cow::Borrowed(template_str);
    }

    match tt.render("t", context) {
        Ok(rendered) => Cow::Owned(rendered),
        Err(_) => Cow::Borrowed(template_str),
    }
}

pub fn render_static_transport<'a>(
    value: &'a str,
    transport: &MotdConfig,
    relay: &RelayConfig,
) -> Cow<'a, str> {
    let ctx = TemplateContext {
        online_player: "{online_player}".to_string(),
        motd_target_addr: transport.upstream_addr.clone(),
        ping_target_addr: transport.ping_target_addr.clone().unwrap_or_default(),
        favicon_target_addr: transport.favicon.target_addr.clone().unwrap_or_default(),
        relay_mode: relay.label().to_string(),
        ping_mode: transport.ping_mode.to_string(),
        favicon_mode: transport.favicon.mode.to_string(),
        upstream_addr: transport.upstream_addr.clone(),
    };
    render(value, &ctx)
}
