use std::fs;
use std::path::Path;

use super::literals::CONFIG_SCHEMA_DIRECTIVE;
use super::schema_types::ConfigFile;

pub(crate) const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:25565";
pub(crate) const DEFAULT_MOTD_UPSTREAM_ADDR: &str = "mc.hypixel.net:25565";
pub(crate) const DEFAULT_API_TARGET_ADDR: &str = "mc.hypixel.net:25565";
pub(crate) const DEFAULT_LOCAL_MOTD_JSON: &str = "{\"version\":{\"name\":\"Proxy\",\"protocol\":-1},\"players\":{\"max\":100,\"online\":%ONLINE_PLAYER%,\"sample\":[{\"name\":\"Welcome to Proxy\",\"id\":\"00000000-0000-0000-0000-000000000001\"},{\"name\":\"Online: %ONLINE_PLAYER%\",\"id\":\"00000000-0000-0000-0000-000000000002\"}]},\"description\":{\"text\":\"Hello from proxy\"}}";
pub(crate) const DEFAULT_FIRST_PACKET_TIMEOUT_MS: u64 = 5_000;
pub(crate) const DEFAULT_KEEPALIVE_SECS: u64 = 30;
pub(crate) const DEFAULT_API_TIMEOUT_MS: u64 = 3_000;
pub(crate) const DEFAULT_API_TRAFFIC_INTERVAL_MS: u64 = 5_000;
pub(crate) const DEFAULT_MOTD_UPSTREAM_PING_TIMEOUT_MS: u64 = 1_500;
pub(crate) const DEFAULT_MOTD_STATUS_CACHE_TTL_MS: u64 = 1_000;
pub(crate) const DEFAULT_STATS_LOG_INTERVAL_SECS: u64 = 10;
pub(crate) const DEFAULT_CONNECTION_ID_PREFIX: &str = "debug";

pub struct ConfigDefaults;

impl ConfigDefaults {
    pub fn file() -> ConfigFile {
        ConfigFile::default()
    }

    pub fn write_if_missing(path: &Path) -> Result<(), String> {
        if path.exists() {
            return Ok(());
        }

        fs::write(path, Self::render_toml()?)
            .map_err(|error| format!("failed to write default config {}: {error}", path.display()))
    }

    pub fn render_toml() -> Result<String, String> {
        let content = toml::to_string_pretty(&Self::file())
            .map_err(|error| format!("failed to serialize default config: {error}"))?;
        Ok(format!("{CONFIG_SCHEMA_DIRECTIVE}\n\n{content}"))
    }
}
