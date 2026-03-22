use std::io;
use std::net::TcpStream;

use tracing::info;

use crate::minecraft::HandshakeInfo;

use super::config::{Config, OutboundConfig, SocketOptions};
use super::socket::apply_stream_options;

#[derive(Clone, Debug)]
pub struct SelectedOutbound {
    pub name: String,
    pub target_addr: String,
    pub rewrite_addr: String,
    pub socket_options: SocketOptions,
}

pub fn select_outbound(config: &Config, handshake: &HandshakeInfo) -> SelectedOutbound {
    let requested_host = normalize_host(&handshake.server_address);

    if let Some(route) = config
        .outbounds
        .iter()
        .find(|route| route.match_host.as_deref() == Some(requested_host.as_str()))
    {
        info!(
            requested_host = %requested_host,
            selected_outbound = %route.outbound.name,
            target_addr = %route.outbound.target_addr,
            rewrite_addr = %route.outbound.rewrite_addr,
            "matched outbound route"
        );
        return SelectedOutbound::from(route.outbound.clone());
    }

    let fallback = config
        .outbounds
        .iter()
        .find(|route| route.match_host.is_none())
        .expect("validated config should include one fallback outbound");

    info!(
        requested_host = %requested_host,
        selected_outbound = %fallback.outbound.name,
        target_addr = %fallback.outbound.target_addr,
        rewrite_addr = %fallback.outbound.rewrite_addr,
        "using fallback outbound"
    );
    SelectedOutbound::from(fallback.outbound.clone())
}

pub fn connect(selected: &SelectedOutbound) -> io::Result<TcpStream> {
    let stream = TcpStream::connect(&selected.target_addr)?;
    apply_stream_options(&stream, &selected.socket_options)?;
    Ok(stream)
}

fn normalize_host(host: &str) -> String {
    let clean = host.split('\0').next().unwrap_or(host);
    clean.trim_end_matches('.').to_ascii_lowercase()
}

impl From<OutboundConfig> for SelectedOutbound {
    fn from(value: OutboundConfig) -> Self {
        Self {
            name: value.name,
            target_addr: value.target_addr,
            rewrite_addr: value.rewrite_addr,
            socket_options: value.socket_options,
        }
    }
}
