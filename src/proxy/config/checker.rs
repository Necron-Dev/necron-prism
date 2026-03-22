use super::types::{
    ApiMode, Config, API_MODE_HTTP, API_MODE_MOCK, CONFIG_SCHEMA_DIRECTIVE,
    MOTD_FAVICON_MODE_OVERRIDE, MOTD_MODE_UPSTREAM,
};

pub struct ConfigChecker;

impl ConfigChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, config: &Config) -> Result<(), String> {
        if matches!(config.api.mode, ApiMode::Http) && config.api.base_url.is_none() {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode={API_MODE_HTTP} requires api.base_url"
            ));
        }

        if matches!(config.api.mode, ApiMode::Mock) && config.api.base_url.is_some() {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode={API_MODE_MOCK} does not use api.base_url"
            ));
        }

        if matches!(config.transport.motd.mode, super::types::MotdMode::Upstream)
            && config.transport.motd.upstream_addr.is_none()
        {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.mode={MOTD_MODE_UPSTREAM} requires transport.motd.upstream_addr"
            ));
        }

        if matches!(config.api.mode, ApiMode::Mock)
            && config.api.mock.connection_id_prefix.is_empty()
        {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode={API_MODE_MOCK} requires a non-empty api.mock.connection_id_prefix"
            ));
        }

        if let super::types::MotdFaviconMode::Override(value) = &config.transport.motd.favicon {
            if value.is_empty() {
                return Err(format!(
                    "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.favicon.mode={MOTD_FAVICON_MODE_OVERRIDE} requires a non-empty favicon.value"
                ));
            }
        }

        Ok(())
    }
}
