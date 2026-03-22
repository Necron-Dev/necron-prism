use std::io::Write;
use std::net::{Shutdown, TcpStream};

use crate::minecraft::{
    decode_ping_request, ping_response_packet, HandshakeInfo, PacketIo, ProtocolError,
    MAX_STATUS_PACKET_SIZE,
};

use super::rewrite::rewrite_json;
use super::service::MotdService;
use super::upstream::UpstreamStatusSession;
use crate::proxy::config::{MotdFaviconMode, MotdMode, StatusPingMode, TransportConfig};
use crate::proxy::players::PlayerRegistry;
use crate::proxy::template;

pub struct StatusContext<'a> {
    transport: &'a TransportConfig,
    handshake: &'a HandshakeInfo,
    service: &'a MotdService,
}

impl<'a> StatusContext<'a> {
    pub fn new(
        transport: &'a TransportConfig,
        handshake: &'a HandshakeInfo,
        service: &'a MotdService,
    ) -> Self {
        Self {
            transport,
            handshake,
            service,
        }
    }

    pub fn open_upstream(&self) -> Result<Option<UpstreamStatusSession>, ProtocolError> {
        let Some(upstream_addr) = self.transport.motd.upstream_addr.as_deref() else {
            return Ok(None);
        };

        let needs_upstream =
            self.transport.motd.mode == MotdMode::Upstream || self.is_local_favicon_passthrough();
        if !needs_upstream {
            return Ok(None);
        }

        UpstreamStatusSession::connect(
            upstream_addr,
            upstream_addr,
            self.handshake,
            self.transport.motd.upstream_ping_timeout,
            self.transport.motd.status_cache_ttl,
            self.service,
        )
        .map(Some)
    }

    pub fn build_json(
        &self,
        players: &PlayerRegistry,
        upstream: Option<&mut UpstreamStatusSession>,
    ) -> Result<String, ProtocolError> {
        let base_json = match self.transport.motd.mode {
            MotdMode::Local => template::render(
                self.transport.motd.local_json.as_deref().unwrap_or("{}"),
                players,
            ),
            MotdMode::Upstream => upstream
                .ok_or_else(|| ProtocolError::decode("missing upstream MOTD session"))?
                .read_status_json()?
                .to_owned(),
        };

        let favicon_source = if self.is_local_favicon_passthrough() {
            self.transport
                .motd
                .upstream_addr
                .as_deref()
                .and_then(|upstream_addr| {
                    self.service.read_cached_status(
                        upstream_addr,
                        upstream_addr,
                        self.transport.motd.status_cache_ttl,
                    )
                })
        } else {
            None
        };

        Ok(rewrite_json(
            &base_json,
            self.transport.motd.protocol_mode,
            self.handshake.protocol_version,
            self.transport.motd.rewrite.as_ref(),
            &self.transport.motd.favicon,
            favicon_source.as_deref(),
        ))
    }

    pub fn finish(
        &self,
        packet_io: &mut PacketIo,
        client: &mut TcpStream,
        mut upstream: Option<&mut UpstreamStatusSession>,
    ) -> Result<StatusOutcome, ProtocolError> {
        match self.transport.motd.ping_mode {
            StatusPingMode::Disconnect => {
                client.shutdown(Shutdown::Both)?;
                Ok(StatusOutcome::default())
            }
            StatusPingMode::ZeroMs => send_pong(client, 0, 0, None),
            StatusPingMode::Passthrough => {
                let ping_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE)?;
                let payload = decode_ping_request(&ping_request)?;
                send_pong(client, payload, ping_request.wire_len, None)
            }
            StatusPingMode::UpstreamTcp => {
                let ping_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE)?;
                let client_payload = decode_ping_request(&ping_request)?;
                let (payload, measured_ms) = match upstream.as_deref_mut() {
                    Some(session) => session.ping(client_payload),
                    None => {
                        let upstream_addr = self
                            .transport
                            .motd
                            .upstream_addr
                            .as_deref()
                            .ok_or_else(|| {
                                ProtocolError::decode("missing MOTD upstream address")
                            })?;
                        UpstreamStatusSession::connect(
                            upstream_addr,
                            upstream_addr,
                            self.handshake,
                            self.transport.motd.upstream_ping_timeout,
                            self.transport.motd.status_cache_ttl,
                            self.service,
                        )?
                        .ping(client_payload)
                    }
                }?;
                send_pong(client, payload, ping_request.wire_len, Some(measured_ms))
            }
        }
    }

    fn is_local_favicon_passthrough(&self) -> bool {
        self.transport.motd.mode == MotdMode::Local
            && matches!(self.transport.motd.favicon, MotdFaviconMode::Passthrough)
            && self.transport.motd.local_json.is_some()
    }
}

#[derive(Default)]
pub struct StatusOutcome {
    pub ping_request_bytes: usize,
    pub pong_bytes: usize,
    pub pong_payload: Option<u64>,
    pub upstream_ping_ms: Option<u32>,
}

fn send_pong(
    client: &mut TcpStream,
    payload: u64,
    ping_request_bytes: usize,
    upstream_ping_ms: Option<u32>,
) -> Result<StatusOutcome, ProtocolError> {
    let pong = ping_response_packet(payload)?;
    client.write_all(&pong)?;

    Ok(StatusOutcome {
        ping_request_bytes,
        pong_bytes: pong.len(),
        pong_payload: Some(payload),
        upstream_ping_ms,
    })
}
