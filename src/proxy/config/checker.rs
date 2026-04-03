use super::literals::CONFIG_SCHEMA_DIRECTIVE;
use super::types::{ApiMode, Config};
use anyhow::{anyhow, Result};

pub struct ConfigChecker;

impl ConfigChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, config: &Config) -> Result<()> {
        if matches!(config.api.mode, ApiMode::Http) && config.api.base_url.is_none() {
            return Err(anyhow!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode=http requires api.base_url"
            ));
        }

        if matches!(config.api.mode, ApiMode::Mock) && config.api.base_url.is_some() {
            return Err(anyhow!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode=mock does not use api.base_url"
            ));
        }

        if matches!(config.transport.motd.mode, super::types::MotdMode::Upstream)
            && config.transport.motd.upstream_addr.is_none()
        {
            return Err(anyhow!(
                "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.mode=upstream requires transport.motd.upstream_addr"
            ));
        }

        if matches!(
            config.transport.motd.ping_mode,
            super::types::StatusPingMode::Passthrough
        ) && config.transport.motd.ping.target_addr.is_none()
            && config.transport.motd.upstream_addr.is_none()
        {
            return Err(anyhow!(
                "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.ping_mode=passthrough requires transport.motd.ping.target_addr or transport.motd.upstream_addr"
            ));
        }

        if matches!(
            config.transport.motd.favicon.mode,
            super::types::MotdFaviconMode::Path
        ) && config.transport.motd.favicon.path.is_none()
        {
            return Err(anyhow!(
                "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.favicon.mode=path requires transport.motd.favicon.path"
            ));
        }

        if matches!(
            config.transport.motd.favicon.mode,
            super::types::MotdFaviconMode::Passthrough
        ) && config.transport.motd.favicon.target_addr.is_none()
            && config.transport.motd.upstream_addr.is_none()
        {
            return Err(anyhow!(
                "{CONFIG_SCHEMA_DIRECTIVE}\ntransport.motd.favicon.mode=passthrough requires transport.motd.favicon.target_addr or transport.motd.upstream_addr"
            ));
        }

        if matches!(config.api.mode, ApiMode::Mock)
            && config.api.mock.connection_id_prefix.is_empty()
        {
            return Err(anyhow!(
                "{CONFIG_SCHEMA_DIRECTIVE}\napi.mode=mock requires a non-empty api.mock.connection_id_prefix"
            ));
        }

        Ok(())
    }
}
