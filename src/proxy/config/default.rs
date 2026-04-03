use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::literals::CONFIG_SCHEMA_DIRECTIVE;
use super::schema_types::ConfigFile;

pub(crate) const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:25565";
pub(crate) const DEFAULT_MOTD_UPSTREAM_ADDR: &str = "mc.hypixel.net:25565";
pub(crate) const DEFAULT_API_TARGET_ADDR: &str = "mc.hypixel.net:25565";
pub(crate) const DEFAULT_LOCAL_MOTD_JSON: &str = "{\"version\":{\"name\":\"§bnecron-prism §7status\",\"protocol\":-1},\"players\":{\"max\":100,\"online\":%ONLINE_PLAYER%,\"sample\":[{\"name\":\"§7mode §8> §f%RELAY_MODE%\",\"id\":\"00000000-0000-0000-0000-000000000001\"},{\"name\":\"§7ping §8> §b%PING_MODE%\",\"id\":\"00000000-0000-0000-0000-000000000002\"},{\"name\":\"§7target §8> §f%MOTD_TARGET_ADDR%\",\"id\":\"00000000-0000-0000-0000-000000000003\"}]},\"description\":{\"text\":\"§bnecron-prism §8» §fclean minecraft relay\\n§7online §f%ONLINE_PLAYER% §8| §7favicon §f%FAVICON_MODE% §8| §7ping §b%PING_MODE%\"}}";
pub(crate) const DEFAULT_FIRST_PACKET_TIMEOUT_MS: u64 = 5_000;
pub(crate) const DEFAULT_KEEPALIVE_SECS: u64 = 30;
pub(crate) const DEFAULT_API_TIMEOUT_MS: u64 = 3_000;
pub(crate) const DEFAULT_API_TRAFFIC_INTERVAL_MS: u64 = 5_000;
pub(crate) const DEFAULT_MOTD_UPSTREAM_PING_TIMEOUT_MS: u64 = 1_500;
pub(crate) const DEFAULT_MOTD_STATUS_CACHE_TTL_MS: u64 = 1_000;
pub(crate) const DEFAULT_STATS_LOG_INTERVAL_SECS: u64 = 10;
pub(crate) const DEFAULT_CONNECTION_ID_PREFIX: &str = "debug";
pub(crate) const DEFAULT_LOG_ASYNC_ENABLED: bool = true;

pub struct ConfigDefaults;

impl ConfigDefaults {
    pub fn file() -> ConfigFile {
        ConfigFile::default()
    }

    pub fn write_if_missing(path: &Path) -> Result<()> {
        if path.exists() {
            return Ok(());
        }

        fs::write(path, Self::render_toml()?)
            .with_context(|| format!("failed to write default config {}", path.display()))
    }

    pub fn render_toml() -> Result<String> {
        let content =
            toml::to_string_pretty(&Self::file()).context("failed to serialize default config")?;
        let mut rendered = String::with_capacity(CONFIG_SCHEMA_DIRECTIVE.len() + content.len() + 2);
        rendered.push_str(CONFIG_SCHEMA_DIRECTIVE);
        rendered.push_str("\n\n");
        rendered.push_str(&content);
        Ok(rendered)
    }
}
