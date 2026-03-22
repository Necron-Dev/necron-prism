use std::path::PathBuf;
use std::time::Duration;

use regex::Regex;

#[derive(Clone, Debug)]
pub struct Config {
    pub inbound: InboundConfig,
    pub outbounds: Vec<OutboundRoute>,
    pub transport: TransportConfig,
    pub stats_log_interval: Option<Duration>,
    pub source_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct InboundConfig {
    pub listen_addr: String,
    pub first_packet_timeout: Duration,
    pub socket_options: SocketOptions,
}

#[derive(Clone, Debug)]
pub struct OutboundConfig {
    pub name: String,
    pub target_addr: String,
    pub rewrite_addr: String,
    pub socket_options: SocketOptions,
}

#[derive(Clone, Debug)]
pub struct OutboundRoute {
    pub match_host: Option<String>,
    pub outbound: OutboundConfig,
}

#[derive(Clone, Debug)]
pub struct TransportConfig {
    pub motd: MotdConfig,
    pub kick_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MotdConfig {
    pub mode: MotdMode,
    pub local_json: Option<String>,
    pub protocol_mode: MotdProtocolMode,
    pub ping_mode: StatusPingMode,
    pub upstream_ping_timeout: Duration,
    pub rewrite: Option<MotdRewrite>,
    pub favicon: MotdFaviconMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotdMode {
    Local,
    Upstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotdProtocolMode {
    Client,
    NegativeOne,
    Fixed(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusPingMode {
    Passthrough,
    ZeroMs,
    UpstreamTcp,
    Disconnect,
}

#[derive(Clone, Debug)]
pub struct MotdRewrite {
    pub description_pattern: Option<Regex>,
    pub description_replacement: Option<String>,
    pub favicon_pattern: Option<Regex>,
    pub favicon_replacement: Option<String>,
}

#[derive(Clone, Debug)]
pub enum MotdFaviconMode {
    Passthrough,
    Override(String),
    Remove,
}

impl MotdFaviconMode {
    #[allow(dead_code)]
    pub fn is_passthrough(&self) -> bool {
        matches!(self, Self::Passthrough)
    }
}

#[derive(Clone, Debug)]
pub struct SocketOptions {
    pub tcp_nodelay: bool,
    pub keepalive: Option<Duration>,
    pub recv_buffer_size: Option<usize>,
    pub send_buffer_size: Option<usize>,
    pub reuse_port: bool,
}

impl Default for SocketOptions {
    fn default() -> Self {
        Self {
            tcp_nodelay: true,
            keepalive: Some(Duration::from_secs(30)),
            recv_buffer_size: None,
            send_buffer_size: None,
            reuse_port: false,
        }
    }
}
