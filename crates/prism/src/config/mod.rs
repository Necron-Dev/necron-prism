mod default;
mod literals;
mod schema;
pub mod types;

pub use types::*;

use self::default::ConfigDefaults;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tracing::warn;

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
            let mut suppressed = Vec::new();

            if socket.multipath_tcp {
                socket.multipath_tcp = false;
                suppressed.push("multipath_tcp");
            }
            if socket.tcp_fastopen {
                socket.tcp_fastopen = false;
                suppressed.push("tcp_fastopen");
            }
            if socket.tcp_fastopen_queue.is_some() {
                socket.tcp_fastopen_queue = None;
                suppressed.push("tcp_fastopen_queue");
            }
            if socket.tcp_quickack {
                socket.tcp_quickack = false;
                suppressed.push("tcp_quickack");
            }
            if socket.ip_tos.is_some() {
                socket.ip_tos = None;
                suppressed.push("ip_tos");
            }
            if socket.congestion_control.is_some() {
                socket.congestion_control = None;
                suppressed.push("congestion_control");
            }
            if socket.bind_interface.is_some() {
                socket.bind_interface = None;
                suppressed.push("bind_interface");
            }
            if socket.fwmark.is_some() {
                socket.fwmark = None;
                suppressed.push("fwmark");
            }
            if socket.reuse_port {
                socket.reuse_port = false;
                suppressed.push("reuse_port");
            }
            if relay.mode != RelayMode::Async {
                relay.mode = RelayMode::Async;
                suppressed.push("relay.mode");
            }

            if !suppressed.is_empty() {
                warn!(
                    options = ?suppressed,
                    reason = "not available on non-Linux platforms",
                    "config options force-disabled"
                );
            }
        }

        #[cfg(target_os = "linux")]
        {
            #[cfg(not(feature = "linux-accel"))]
            {
                let socket = &mut config.network.socket;
                let relay = &mut config.network.relay;
                let mut suppressed = Vec::new();

                if socket.multipath_tcp {
                    socket.multipath_tcp = false;
                    suppressed.push("multipath_tcp");
                }
                if socket.tcp_fastopen {
                    socket.tcp_fastopen = false;
                    suppressed.push("tcp_fastopen");
                }
                if socket.tcp_fastopen_queue.is_some() {
                    socket.tcp_fastopen_queue = None;
                    suppressed.push("tcp_fastopen_queue");
                }
                if socket.tcp_quickack {
                    socket.tcp_quickack = false;
                    suppressed.push("tcp_quickack");
                }
                if socket.ip_tos.is_some() {
                    socket.ip_tos = None;
                    suppressed.push("ip_tos");
                }
                if socket.congestion_control.is_some() {
                    socket.congestion_control = None;
                    suppressed.push("congestion_control");
                }
                if socket.bind_interface.is_some() {
                    socket.bind_interface = None;
                    suppressed.push("bind_interface");
                }
                if socket.fwmark.is_some() {
                    socket.fwmark = None;
                    suppressed.push("fwmark");
                }
                if relay.mode != RelayMode::Async {
                    relay.mode = RelayMode::Async;
                    suppressed.push("relay.mode");
                }

                if !suppressed.is_empty() {
                    warn!(
                        options = ?suppressed,
                        reason = "linux-accel feature not enabled",
                        "config options force-disabled"
                    );
                }
            }
        }
    }
}

#[cfg(feature = "schema")]
pub use schema::write_schema_file;

#[cfg(test)]
mod test;
