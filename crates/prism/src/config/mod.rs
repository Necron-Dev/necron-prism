mod default;
mod literals;
mod schema;
pub mod types;

pub use types::*;

use self::default::ConfigDefaults;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_default() -> Result<Config> {
        Self::load_from_path(Path::new("config.toml"))
    }

    pub fn load_from_path(path: &Path) -> Result<Config> {
        ConfigDefaults::write_if_missing(path)?;

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;

        let mut config = toml::from_str::<Config>(&content)
            .with_context(|| format!("failed to parse TOML config {}", path.display()))?;

        config.source_path = path.to_path_buf();
        config.requested_relay = config.network.relay.clone();
        Self::canonicalize_runtime_config(&mut config);
        config.validate()?;
        Ok(config)
    }

    fn canonicalize_runtime_config(config: &mut Config) {
        #[cfg(not(target_os = "linux"))]
        {
            let socket = &mut config.network.socket;
            let relay = &mut config.network.relay;

            socket.multipath_tcp = false;
            socket.tcp_fastopen = false;
            socket.tcp_fastopen_queue = None;
            socket.tcp_quickack = false;
            socket.ip_tos = None;
            socket.congestion_control = None;
            socket.bind_interface = None;
            socket.fwmark = None;
            socket.reuse_port = false;

            relay.mode = RelayMode::Async;
        }

        #[cfg(target_os = "linux")]
        {
            #[cfg(not(feature = "linux-accel"))]
            {
                let socket = &mut config.network.socket;
                let relay = &mut config.network.relay;

                socket.tcp_fastopen = false;
                socket.tcp_fastopen_queue = None;
                socket.tcp_quickack = false;
                socket.ip_tos = None;
                socket.congestion_control = None;
                socket.bind_interface = None;
                socket.fwmark = None;

                relay.mode = RelayMode::Async;
            }
        }
    }
}

#[cfg(feature = "schema")]
pub use schema::write_schema_file;

#[cfg(test)]
mod test;
