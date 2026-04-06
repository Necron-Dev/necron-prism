use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::minecraft::{
    decode_ping_request, ping_response_packet, HandshakeInfo, PacketIo, MAX_STATUS_PACKET_SIZE,
};

use super::rewrite::rewrite_json;
use super::service::MotdService;
use super::upstream::UpstreamStatusSession;
use crate::proxy::config::{MotdFaviconMode, MotdMode, StatusPingMode, MotdConfig, RelayConfig};
use crate::proxy::players::PlayerRegistry;
use crate::proxy::template::{self, TemplateContext};

pub struct StatusContext<'a> {
    config: &'a MotdConfig,
    relay: &'a RelayConfig,
    handshake: &'a HandshakeInfo,
    service: &'a MotdService,
}

impl<'a> StatusContext<'a> {
    pub fn new(
        config: &'a MotdConfig,
        relay: &'a RelayConfig,
        handshake: &'a HandshakeInfo,
        service: &'a MotdService,
    ) -> Self {
        Self {
            config,
            relay,
            handshake,
            service,
        }
    }

    pub async fn open_upstream(&self) -> anyhow::Result<Option<UpstreamStatusSession>> {
        let Some(target_addr) = self.upstream_target_addr() else {
            return Ok(None);
        };

        let needs_status_json = self.status_response_needs_upstream();
        let needs_ping = self.config.ping_mode == StatusPingMode::Passthrough;

        UpstreamStatusSession::connect(
            target_addr,
            self.rewrite_addr(target_addr),
            self.handshake,
            std::time::Duration::from_millis(self.config.upstream_ping_timeout_ms),
            self.service,
            needs_status_json,
            needs_ping,
        )
        .await
        .map(Some)
    }

    pub async fn build_json(
        &self,
        players: &PlayerRegistry,
        mut upstream: Option<&mut UpstreamStatusSession>,
    ) -> anyhow::Result<String> {
        if let Some(json) = self
            .service
            .render_local_json(self.config, self.relay, self.handshake, players)
            .await
        {
            return Ok(json.as_ref().to_owned());
        }

        let explicit_favicon = self.load_explicit_favicon_data_url().await?;
        let template_context = TemplateContext::for_transport(self.config, self.relay, players);

        let base_json = match self.config.mode {
            MotdMode::Local => {
                template::render(&self.config.local_json, &template_context).into_owned()
            }
            MotdMode::Upstream => upstream
                .as_deref_mut()
                .ok_or_else(|| anyhow::anyhow!("missing upstream MOTD session"))?
                .read_status_json()
                .await?
                .to_owned(),
        };

        let favicon_source = if self.should_passthrough_favicon() {
            match upstream {
                Some(session) => Some(session.read_status_json().await?),
                None => None,
            }
        } else {
            None
        };

        Ok(rewrite_json(
            &base_json,
            self.config.protocol,
            self.handshake.protocol_version,
            &self.config.favicon,
            explicit_favicon.as_deref(),
            favicon_source,
        ))
    }

    pub async fn finish(
        &self,
        packet_io: &mut PacketIo,
        client: &mut tokio::net::TcpStream,
        upstream: Option<&mut UpstreamStatusSession>,
    ) -> anyhow::Result<StatusOutcome> {
        match self.config.ping_mode {
            StatusPingMode::Disconnect => {
                client.shutdown().await?;
                Ok(StatusOutcome::default())
            }
            StatusPingMode::ZeroMs => send_pong(client, 0, 0, None).await,
            StatusPingMode::Local => {
                let ping_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE).await?;
                let payload = decode_ping_request(&ping_request).map_err(anyhow::Error::from)?;
                send_pong(client, payload, ping_request.wire_len, None).await
            }
            StatusPingMode::Passthrough => {
                let ping_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE).await?;
                let client_payload = decode_ping_request(&ping_request).map_err(anyhow::Error::from)?;
                let (payload, measured_ms) = match upstream {
                    Some(session) => session.ping(client_payload).await,
                    None => {
                        let target_addr = self
                            .ping_target_addr()
                            .ok_or_else(|| anyhow::anyhow!("missing MOTD ping target address"))?;
                        UpstreamStatusSession::connect(
                            target_addr,
                            self.rewrite_addr(target_addr),
                            self.handshake,
                            std::time::Duration::from_millis(self.config.upstream_ping_timeout_ms),
                            self.service,
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
        self.config.ping_target_addr.as_deref().or(Some(&self.config.upstream_addr))
    }

    fn favicon_target_addr(&self) -> Option<&str> {
        self.config.favicon.target_addr.as_deref().or(Some(&self.config.upstream_addr))
    }

    fn upstream_target_addr(&self) -> Option<&str> {
        if self.config.mode == MotdMode::Upstream {
            Some(&self.config.upstream_addr)
        } else if self.should_passthrough_favicon() {
            self.favicon_target_addr()
        } else if self.config.ping_mode == StatusPingMode::Passthrough {
            self.ping_target_addr()
        } else {
            None
        }
    }

    fn rewrite_addr<'b>(&'b self, target_addr: &'b str) -> &'b str {
        if !self.config.upstream_addr.is_empty() {
             &self.config.upstream_addr
        } else {
            target_addr
        }
    }

    fn should_passthrough_favicon(&self) -> bool {
        self.config.favicon.mode == MotdFaviconMode::Passthrough
    }

    async fn load_explicit_favicon_data_url(&self) -> anyhow::Result<Option<Arc<str>>> {
        match self.config.favicon.mode {
            MotdFaviconMode::Path => {
                let path = self
                    .config
                    .favicon
                    .path
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("missing MOTD favicon path"))?;
                self.service.read_favicon_data_url(path).await.map(Some)
            }
            _ => Ok(None),
        }
    }

    fn status_response_needs_upstream(&self) -> bool {
        self.config.mode == MotdMode::Upstream || self.should_passthrough_favicon()
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
