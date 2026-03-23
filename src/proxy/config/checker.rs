use super::literals::CONFIG_SCHEMA_DIRECTIVE;
use super::types::{ApiMode, Config};

pub struct ConfigChecker;

impl ConfigChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, config: &Config) -> Result<(), String> {
        if matches!(config.api.mode, ApiMode::Http) && config.api.base_url.is_none() {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode=http requires api.base_url"
            ));
        }

        if matches!(config.api.mode, ApiMode::Mock) && config.api.base_url.is_some() {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode=mock does not use api.base_url"
            ));
        }

        if matches!(config.transport.motd.mode, super::types::MotdMode::Upstream)
            && config.transport.motd.upstream_addr.is_none()
        {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.mode=upstream requires transport.motd.upstream_addr"
            ));
        }

        if matches!(config.api.mode, ApiMode::Mock)
            && config.api.mock.connection_id_prefix.is_empty()
        {
            return Err(format!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode=mock requires a non-empty api.mock.connection_id_prefix"
            ));
        }

        if let super::types::MotdFaviconMode::Override(value) = &config.transport.motd.favicon {
            if value.is_empty() {
                return Err(format!(
                    "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.favicon.mode=override requires a non-empty favicon.value"
                ));
            }
        }

        Ok(())
    }
}
