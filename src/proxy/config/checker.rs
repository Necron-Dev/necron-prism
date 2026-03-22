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

        if matches!(config.transport.motd.mode, MotdMode::Upstream)
            && config.transport.motd.upstream_addr.is_none()
        {
            return Err(
                "transport.motd.mode=upstream requires transport.motd.upstream_addr".to_string(),
            );
        }

        Ok(())
    }
}
