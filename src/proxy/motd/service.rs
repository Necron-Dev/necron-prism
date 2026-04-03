use crate::minecraft::{
    decode_status_request, ping_response_packet, status_response_packet, HandshakeInfo, PacketIo,
    MAX_STATUS_PACKET_SIZE,
};
use crate::proxy::config::{
    MotdFaviconMode, MotdMode, RelayMode, StatusPingMode, TransportConfig,
};
use crate::proxy::players::{PlayerRegistry, PlayerState};
use crate::proxy::template;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tracing::info;

use super::cache::StatusCache;
use super::context::StatusContext;
use super::favicon::FaviconCache;
use super::rewrite::rewrite_json;

#[derive(Clone)]
pub struct MotdService {
    cache: StatusCache,
    favicon_cache: FaviconCache,
    local_json_template: Arc<Option<Arc<str>>>,
    local_favicon_data_url: Arc<Option<Arc<str>>>,
}

impl Default for MotdService {
    fn default() -> Self {
        Self {
            cache: StatusCache::default(),
            favicon_cache: FaviconCache::default(),
            local_json_template: Arc::new(None),
            local_favicon_data_url: Arc::new(None),
        }
    }
}

impl MotdService {
    pub fn new(transport: &TransportConfig, relay_mode: RelayMode) -> anyhow::Result<Self> {
        let (local_json_template, local_favicon_data_url) = prepare_local_motd(transport, relay_mode)?;
        Ok(Self {
            cache: StatusCache::default(),
            favicon_cache: FaviconCache::default(),
            local_json_template: Arc::new(local_json_template),
            local_favicon_data_url: Arc::new(local_favicon_data_url),
        })
    }

    pub async fn serve(
        &self,
        packet_io: &mut PacketIo,
        client: &mut tokio::net::TcpStream,
        transport: &TransportConfig,
        relay_mode: RelayMode,
        handshake: &HandshakeInfo,
        players: &PlayerRegistry,
        connection_id: u64,
    ) -> anyhow::Result<()> {
        let status_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE).await?;
        decode_status_request(&status_request).map_err(anyhow::Error::from)?;

        let context = StatusContext::new(transport, relay_mode, handshake, self);
        let mut upstream = if transport.motd.mode == MotdMode::Upstream
            || transport.motd.favicon.mode == MotdFaviconMode::Passthrough
            || transport.motd.ping_mode == StatusPingMode::Passthrough
        {
            context.open_upstream().await?
        } else {
            None
        };

        let motd_json = context.build_json(players, upstream.as_mut()).await?;
        let mut status_response = status_response_packet(&motd_json).map_err(anyhow::Error::from)?;

        let outcome = match transport.motd.ping_mode {
            StatusPingMode::ZeroMs => {
                let pong = ping_response_packet(0).map_err(anyhow::Error::from)?;
                status_response.extend_from_slice(&pong);
                client.write_all(&status_response).await?;
                super::context::StatusOutcome {
                    ping_request_bytes: 0,
                    pong_bytes: pong.len(),
                    pong_payload: Some(0),
                    upstream_ping_ms: None,
                }
            }
            StatusPingMode::Disconnect => {
                client.write_all(&status_response).await?;
                super::context::StatusOutcome::default()
            }
            _ => {
                client.write_all(&status_response).await?;
                context.finish(packet_io, client, upstream.as_mut()).await?
            }
        };

        players.set_state(connection_id, PlayerState::StatusServedLocally);

        info!(
            motd_mode = ?transport.motd.mode,
            ping_mode = ?transport.motd.ping_mode,
            status_request_bytes = status_request.wire_len,
            motd_response_bytes = status_response.len(),
            ping_request_bytes = outcome.ping_request_bytes,
            pong_bytes = outcome.pong_bytes,
            pong_payload = ?outcome.pong_payload,
            upstream_ping_ms = ?outcome.upstream_ping_ms,
            "served MOTD"
        );

        Ok(())
    }

    pub fn read_cached_status(
        &self,
        target_addr: &str,
        rewrite_addr: &str,
        ttl: std::time::Duration,
    ) -> Option<std::sync::Arc<str>> {
        self.cache.read(target_addr, rewrite_addr, ttl)
    }

    pub fn store_cached_status_arc(
        &self,
        target_addr: &str,
        rewrite_addr: &str,
        json: std::sync::Arc<str>,
    ) {
        self.cache.write_arc(target_addr, rewrite_addr, json)
    }

    pub async fn read_favicon_data_url(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<std::sync::Arc<str>> {
        self.favicon_cache.read_data_url(path).await
    }

    pub fn render_local_json(
        &self,
        transport: &TransportConfig,
        handshake: &HandshakeInfo,
        players: &PlayerRegistry,
    ) -> Option<Arc<str>> {
        if transport.motd.mode != MotdMode::Local {
            return None;
        }

        let template = self.local_json_template.as_ref().as_ref()?;
        let online_players = players.current_online_count().to_string();
        let rendered = template.replace("%ONLINE_PLAYER%", &online_players);
        Some(Arc::<str>::from(rewrite_json(
            &rendered,
            transport.motd.protocol_mode,
            handshake.protocol_version,
            &transport.motd.favicon,
            self.local_favicon_data_url.as_deref(),
            None,
        )))
    }
}

fn prepare_local_motd(
    transport: &TransportConfig,
    relay_mode: RelayMode,
) -> anyhow::Result<(Option<Arc<str>>, Option<Arc<str>>)> {
    if transport.motd.mode != MotdMode::Local {
        return Ok((None, None));
    }

    let local_json_template = Some(Arc::<str>::from(
        template::render_static_transport(&transport.motd.local_json, transport, relay_mode)
            .into_owned(),
    ));

    let local_favicon_data_url = match transport.motd.favicon.mode {
        MotdFaviconMode::Path => {
            let path = transport
                .motd
                .favicon
                .path
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing MOTD favicon path"))?;
            Some(preload_favicon_data_url(path)?)
        }
        _ => None,
    };

    Ok((local_json_template, local_favicon_data_url))
    }

fn preload_favicon_data_url(path: &Path) -> anyhow::Result<Arc<str>> {
    use anyhow::Context;
    use base64::Engine;

    let bytes = std::fs::read(path)
        .with_context(|| format!("read favicon file {} during MOTD preload", path.display()))?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(Arc::<str>::from(format!(
        "data:{};base64,{encoded}",
        mime.essence_str()
    )))
}
