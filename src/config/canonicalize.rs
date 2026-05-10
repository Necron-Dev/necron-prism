use tracing::warn;

use crate::config::{ApiMode, NecronPrismConfig};

pub fn canonicalize_runtime_config(config: &mut NecronPrismConfig) {
    #[cfg(not(target_os = "linux"))]
    {
        use prism::config::{RelayConfig, RelayMode};

        if config.prism.network.relay.mode != RelayMode::Async {
            config.prism.requested_relay = RelayConfig {
                mode: config.prism.network.relay.mode,
            };
            warn!(
                option = "network.relay.mode",
                reason = format!(
                    "{} is only available on Linux",
                    config.prism.network.relay.mode
                ),
                "config option suppressed"
            );
            config.prism.network.relay.mode = RelayMode::Async;
        }
        if config.prism.network.socket.multipath_tcp {
            warn!(
                option = "network.socket.multipath_tcp",
                reason = "MPTCP is only available on Linux",
                "config option suppressed"
            );
            config.prism.network.socket.multipath_tcp = false;
        }
        if config.prism.network.socket.tcp_quickack {
            warn!(
                option = "network.socket.tcp_quickack",
                reason = "TCP_QUICKACK is only available on Linux",
                "config option suppressed"
            );
            config.prism.network.socket.tcp_quickack = false;
        }
        if config.prism.network.socket.ip_tos.is_some() {
            warn!(
                option = "network.socket.ip_tos",
                reason = "IP_TOS is only available on Linux",
                "config option suppressed"
            );
            config.prism.network.socket.ip_tos = None;
        }
        if config.prism.network.socket.congestion_control.is_some() {
            warn!(
                option = "network.socket.congestion_control",
                reason = "TCP_CONGESTION is only available on Linux",
                "config option suppressed"
            );
            config.prism.network.socket.congestion_control = None;
        }
        if config.prism.network.socket.bind_interface.is_some() {
            warn!(
                option = "network.socket.bind_interface",
                reason = "SO_BINDTODEVICE is only available on Linux",
                "config option suppressed"
            );
            config.prism.network.socket.bind_interface = None;
        }
        if config.prism.network.socket.fwmark.is_some() {
            warn!(
                option = "network.socket.fwmark",
                reason = "SO_MARK is only available on Linux",
                "config option suppressed"
            );
            config.prism.network.socket.fwmark = None;
        }
        if config.prism.network.socket.tcp_fastopen {
            warn!(
                option = "network.socket.tcp_fastopen",
                reason = "TCP_FASTOPEN is only available on Linux",
                "config option suppressed"
            );
            config.prism.network.socket.tcp_fastopen = false;
        }
        if config.prism.network.socket.reuse_port {
            warn!(
                option = "network.socket.reuse_port",
                reason = "SO_REUSEPORT is only available on Linux/Unix",
                "config option suppressed"
            );
            config.prism.network.socket.reuse_port = false;
        }
    }

    #[cfg(all(target_os = "linux", not(feature = "linux-accel")))]
    {
        use prism::config::{RelayConfig, RelayMode};

        if config.prism.network.relay.mode != RelayMode::Async {
            config.prism.requested_relay = RelayConfig {
                mode: config.prism.network.relay.mode,
            };
            warn!(
                option = "network.relay.mode",
                reason = format!(
                    "{} requires the linux-accel feature (compiled without it)",
                    config.prism.network.relay.mode
                ),
                "config option suppressed"
            );
            config.prism.network.relay.mode = RelayMode::Async;
        }
        if config.prism.network.socket.multipath_tcp {
            warn!(
                option = "network.socket.multipath_tcp",
                reason = "MPTCP requires the linux-accel feature (compiled without it)",
                "config option suppressed"
            );
            config.prism.network.socket.multipath_tcp = false;
        }
    }

    if config.api.mode == ApiMode::Http && config.api.entry_node_key.is_none() {
        warn!(
            option = "api.entry_node_key",
            reason = "ENTRY_NODE_KEY should be specific when API_MODE is HTTP",
            "config option suppressed"
        );
        config.api.entry_node_key = Some("default".to_string());
    }
}
