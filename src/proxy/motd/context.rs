use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::minecraft::{
    decode_ping_request, ping_response_packet, HandshakeInfo, PacketIo, MAX_STATUS_PACKET_SIZE,
};

use super::rewrite::rewrite_json;
use super::service::MotdService;
use super::upstream::UpstreamStatusSession;
use crate::proxy::config::{MotdFaviconMode, MotdMode, StatusPingMode, TransportConfig};
use crate::proxy::players::PlayerRegistry;
use crate::proxy::template::{self, TemplateContext};

pub struct StatusContext<'a> {
    transport: &'a TransportConfig,
    relay_mode: crate::proxy::config::RelayMode,
    handshake: &'a HandshakeInfo,
    service: &'a MotdService,
}

impl<'a> StatusContext<'a> {
    pub fn new(
        transport: &'a TransportConfig,
        relay_mode: crate::proxy::config::RelayMode,
        handshake: &'a HandshakeInfo,
        service: &'a MotdService,
    ) -> Self {
        Self {
            transport,
            relay_mode,
            handshake,
            service,
        }
    }

    pub async fn open_upstream(&self) -> anyhow::Result<Option<UpstreamStatusSession>> {
        let Some(target_addr) = self.upstream_target_addr() else {
            return Ok(None);
        };

        let plan = self.upstream_plan();
        if !plan.needs_connection() {
            return Ok(None);
        }

        UpstreamStatusSession::connect(
            target_addr,
            self.rewrite_addr(target_addr),
            self.handshake,
            self.transport.motd.upstream_ping_timeout,
            self.service,
            plan.cached_status_json,
            plan.needs_status_json,
            plan.needs_ping,
        )
        .await
        .map(Some)
    }

    pub async fn build_json(
        &self,
        players: &PlayerRegistry,
        mut upstream: Option<&mut UpstreamStatusSession>,
    ) -> anyhow::Result<String> {
        let explicit_favicon = self.load_explicit_favicon_data_url()?;
        let template_context =
            TemplateContext::for_transport(self.transport, self.relay_mode, players);

        let base_json = match self.transport.motd.mode {
            MotdMode::Local => {
                template::render(&self.transport.motd.local_json, &template_context).into_owned()
            }
            MotdMode::Upstream => upstream
                .as_deref_mut()
                .ok_or_else(|| anyhow::anyhow!("missing upstream MOTD session"))?
                .read_status_json()
                .await?
                .to_owned(),
        };

        let favicon_source = if self.should_passthrough_favicon() {
            match upstream.as_deref_mut() {
                Some(session) => Some(session.read_status_json().await?),
                None => None,
            }
        } else {
            None
        };

        Ok(rewrite_json(
            &base_json,
            self.transport.motd.protocol_mode,
            self.handshake.protocol_version,
            &self.transport.motd.favicon,
            explicit_favicon.as_deref(),
            favicon_source,
        ))
    }

    pub async fn finish(
        &self,
        packet_io: &mut PacketIo,
        client: &mut tokio::net::TcpStream,
        mut upstream: Option<&mut UpstreamStatusSession>,
    ) -> anyhow::Result<StatusOutcome> {
        match self.transport.motd.ping_mode {
            StatusPingMode::Disconnect => {
                client.shutdown().await?;
                Ok(StatusOutcome::default())
            }
            StatusPingMode::ZeroMs => send_pong(client, 0, 0, None).await,
            StatusPingMode::Passthrough => {
                let ping_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE).await?;
                let payload = decode_ping_request(&ping_request).map_err(anyhow::Error::from)?;
                send_pong(client, payload, ping_request.wire_len, None).await
            }
            StatusPingMode::UpstreamTcp => {
                let ping_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE).await?;
                let client_payload = decode_ping_request(&ping_request).map_err(anyhow::Error::from)?;
                let (payload, measured_ms) = match upstream.as_deref_mut() {
                    Some(session) => session.ping(client_payload).await,
                    None => {
                        let target_addr = self
                            .ping_target_addr()
                            .ok_or_else(|| anyhow::anyhow!("missing MOTD ping target address"))?;
                        UpstreamStatusSession::connect(
                            target_addr,
                            self.rewrite_addr(target_addr),
                            self.handshake,
                            self.transport.motd.upstream_ping_timeout,
                            self.service,
                            None,
                            true,
                            true,
                        )
                        .await?
                        .ping(client_payload)
                        .await
                    }
                }?;
                send_pong(client, payload, ping_request.wire_len, Some(measured_ms)).await
            }
        }
    }

    fn ping_target_addr(&self) -> Option<&str> {
        self.transport.motd.ping.target_addr.as_deref().or(self
            .transport
            .motd
            .upstream_addr
            .as_deref())
    }

    fn favicon_target_addr(&self) -> Option<&str> {
        self.transport.motd.favicon.target_addr.as_deref().or(self
            .transport
            .motd
            .upstream_addr
            .as_deref())
    }

    fn upstream_target_addr(&self) -> Option<&str> {
        if self.transport.motd.mode == MotdMode::Upstream {
            self.transport.motd.upstream_addr.as_deref()
        } else if self.should_passthrough_favicon() {
            self.favicon_target_addr()
        } else if self.transport.motd.ping_mode == StatusPingMode::UpstreamTcp {
            self.ping_target_addr()
        } else {
            None
        }
    }

    fn rewrite_addr<'b>(&'b self, target_addr: &'b str) -> &'b str {
        self.transport
            .motd
            .upstream_addr
            .as_deref()
            .unwrap_or(target_addr)
    }

    fn should_passthrough_favicon(&self) -> bool {
        self.transport.motd.favicon.mode == MotdFaviconMode::Passthrough
            && self.favicon_target_addr().is_some()
    }

    fn load_explicit_favicon_data_url(&self) -> anyhow::Result<Option<Arc<str>>> {
        match self.transport.motd.favicon.mode {
            MotdFaviconMode::Path => {
                let path = self
                    .transport
                    .motd
                    .favicon
                    .path
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("missing MOTD favicon path"))?;
                self.service.read_favicon_data_url(path).map(Some)
            }
            _ => Ok(None),
        }
    }

    fn upstream_plan(&self) -> UpstreamPlan {
        let needs_status_json =
            self.transport.motd.mode == MotdMode::Upstream || self.should_passthrough_favicon();
        let needs_ping = self.transport.motd.ping_mode == StatusPingMode::UpstreamTcp;
        let cached_status_json = if needs_status_json {
            self.upstream_target_addr().and_then(|target_addr| {
                self.service.read_cached_status(
                    target_addr,
                    self.rewrite_addr(target_addr),
                    self.transport.motd.status_cache_ttl,
                )
            })
        } else {
            None
        };

        UpstreamPlan {
            cached_status_json,
            needs_status_json,
            needs_ping,
        }
    }
}

struct UpstreamPlan {
    cached_status_json: Option<Arc<str>>,
    needs_status_json: bool,
    needs_ping: bool,
}

impl UpstreamPlan {
    fn needs_connection(&self) -> bool {
        self.needs_ping || (self.needs_status_json && self.cached_status_json.is_none())
    }
}

#[derive(Default)]
pub struct StatusOutcome {
    pub ping_request_bytes: usize,
    pub pong_bytes: usize,
    pub pong_payload: Option<u64>,
    pub upstream_ping_ms: Option<u32>,
}

async fn send_pong(
    client: &mut tokio::net::TcpStream,
    payload: u64,
    ping_request_bytes: usize,
    upstream_ping_ms: Option<u32>,
) -> anyhow::Result<StatusOutcome> {
    let pong = ping_response_packet(payload).map_err(anyhow::Error::from)?;
    client.write_all(&pong).await?;

    Ok(StatusOutcome {
        ping_request_bytes,
        pong_bytes: pong.len(),
        pong_payload: Some(payload),
        upstream_ping_ms,
    })
}
