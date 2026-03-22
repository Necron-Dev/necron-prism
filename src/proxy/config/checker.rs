use super::types::{ApiMode, Config, MotdMode};

pub struct ConfigChecker;

impl ConfigChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, config: &Config) -> Result<(), String> {
        if matches!(config.api.mode, ApiMode::Http) && config.api.base_url.is_none() {
            return Err("api.mode=http requires api.base_url".to_string());
        }

        if matches!(config.api.mode, ApiMode::Mock) && config.api.base_url.is_some() {
            return Err("api.mode=mock does not use api.base_url".to_string());
        }

        if matches!(config.transport.motd.mode, MotdMode::Upstream)
            && config.transport.motd.upstream_addr.is_none()
        {
            return Err(
                "transport.motd.mode=upstream requires transport.motd.upstream_addr".to_string(),
            );
        }

        if matches!(config.api.mode, ApiMode::Mock)
            && config.api.mock.connection_id_prefix.is_empty()
        {
            return Err("api.mock.connection_id_prefix cannot be empty".to_string());
        }

        if let super::types::MotdFaviconMode::Override(value) = &config.transport.motd.favicon {
            if value.is_empty() {
                return Err(
                    "transport.motd.favicon.mode=override requires a non-empty favicon.value"
                        .to_string(),
                );
            }
        }

        Ok(())
    }
}
