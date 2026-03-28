use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub inbound: InboundConfig,
    pub transport: TransportConfig,
    pub relay: RelayConfig,
    pub api: ApiConfig,
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
pub struct TransportConfig {
    pub motd: MotdConfig,
}

#[derive(Clone, Debug)]
pub struct RelayConfig {
    pub mode: RelayMode,
}

#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub mode: ApiMode,
    pub base_url: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout: Duration,
    pub traffic_interval: Duration,
    pub mock: MockApiConfig,
}

#[derive(Clone, Debug)]
pub struct MockApiConfig {
    pub target_addr: String,
    pub rewrite_addr: String,
    pub connection_id_prefix: String,
    pub kick_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiMode {
    Http,
    Mock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayMode {
    Standard,
    LinuxSplice,
}

#[derive(Clone, Debug)]
pub struct MotdConfig {
    pub mode: MotdMode,
    pub local_json: Option<String>,
    pub upstream_addr: Option<String>,
    pub protocol_mode: MotdProtocolMode,
    pub ping_mode: StatusPingMode,
    pub upstream_ping_timeout: Duration,
    pub status_cache_ttl: Duration,
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
pub enum MotdFaviconMode {
    Passthrough,
    Remove,
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
