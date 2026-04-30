use crate::config::{MotdConfig, RelayConfig};
use rayon::prelude::*;
use serde::Serialize;
use std::borrow::Cow;

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
    pub fn for_transport(transport: &MotdConfig, relay: &RelayConfig, online_count: i32) -> Self {
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
            online_player: online_count.to_string(),
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

    let replacements = [
        ("{online_player}", context.online_player.as_str()),
        ("{motd_target_addr}", context.motd_target_addr.as_str()),
        ("{ping_target_addr}", context.ping_target_addr.as_str()),
        (
            "{favicon_target_addr}",
            context.favicon_target_addr.as_str(),
        ),
        ("{relay_mode}", context.relay_mode.as_str()),
        ("{ping_mode}", context.ping_mode.as_str()),
        ("{favicon_mode}", context.favicon_mode.as_str()),
        ("{upstream_addr}", context.upstream_addr.as_str()),
    ];

    // Replacements must be sequential because each one modifies the string
    let active_replacements: Vec<_> = replacements
        .par_iter()
        .filter(|(placeholder, _)| template_str.contains(placeholder))
        .map(|(placeholder, value)| (*placeholder, *value))
        .collect();

    if active_replacements.is_empty() {
        return Cow::Borrowed(template_str);
    }

    let mut rendered = template_str.to_owned();
    for (placeholder, value) in active_replacements {
        rendered = rendered.replace(placeholder, value);
    }

    Cow::Owned(rendered)
}
