use necron_prism_minecraft::{
    decode_status_request, encode_raw_frame, ping_response_packet, status_response_packet, HandshakeInfo, PacketIo,
    MAX_STATUS_PACKET_SIZE,
};
use crate::config::{MotdConfig, MotdFaviconMode, MotdMode, RelayConfig, StatusPingMode};
use crate::template;
use crate::session::ConnectionSession;

use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use super::context::StatusContext;
use super::rewrite::rewrite_json;

#[derive(Clone, Default)]
pub struct MotdService {}

impl MotdService {
    pub fn new() -> Self {
        Self {}
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn serve(
        &self,
        packet_io: &mut PacketIo,
        client: &mut tokio::net::TcpStream,
        motd_config: &MotdConfig,
        relay: &RelayConfig,
        online_count: i32,
        handshake: &HandshakeInfo,
        session: &ConnectionSession,
    ) -> anyhow::Result<()> {
        let _guard = session.enter_stage("MOTD");
        let status_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE).await?;
        decode_status_request(&status_request).map_err(anyhow::Error::from)?;

        let status_request_wire = encode_raw_frame(&status_request).map_err(anyhow::Error::from)?;

        let context = StatusContext::new(motd_config, relay, handshake, self);
        let mut upstream = if motd_config.mode == MotdMode::Upstream
            || motd_config.favicon.mode == MotdFaviconMode::Passthrough
            || motd_config.ping_mode == StatusPingMode::Passthrough
        {
            context.open_upstream(&status_request_wire).await?
        } else {
            None
        };

        let motd_json = context.build_json(online_count, upstream.as_mut()).await?;
        let mut status_response = status_response_packet(&motd_json).map_err(anyhow::Error::from)?;

        let outcome = match motd_config.ping_mode {
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

        debug!(
            motd_mode = ?motd_config.mode,
            ping_mode = ?motd_config.ping_mode,
            status_request_bytes = status_request.wire_len,
            motd_response_bytes = status_response.len(),
            ping_request_bytes = outcome.ping_request_bytes,
            pong_bytes = outcome.pong_bytes,
            pong_payload = ?outcome.pong_payload,
            upstream_ping_ms = ?outcome.upstream_ping_ms,
            "[CONNECT/MOTD] status served to client"
        );

        Ok(())
    }

    pub async fn read_favicon_data_url(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<std::sync::Arc<str>> {
        use base64::Engine;
        use anyhow::Context;

        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("read favicon file {}", path.display()))?;
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let data_url = Arc::<str>::from(format!("data:{};base64,{encoded}", mime.essence_str()));

        Ok(data_url)
    }

    pub async fn render_local_json(
        &self,
        config: &MotdConfig,
        relay: &RelayConfig,
        handshake: &HandshakeInfo,
        online_count: i32,
    ) -> Option<Arc<str>> {
        if config.mode != MotdMode::Local {
            return None;
        }

        let context = template::TemplateContext::for_transport(config, relay, online_count);
        let final_text = template::render(&config.local_json, &context).into_owned();

        let favicon_data_url = match config.favicon.mode {
            MotdFaviconMode::Path => {
                let path = config.favicon.path.as_deref()?;
                self.read_favicon_data_url(Path::new(path)).await.ok()
            }
            _ => None,
        };

        Some(Arc::<str>::from(rewrite_json(
            &final_text,
            config.protocol,
            handshake.protocol_version,
            &config.favicon,
            favicon_data_url.as_deref(),
            None,
        )))
    }
}
