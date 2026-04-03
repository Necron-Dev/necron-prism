use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use tokio::io::AsyncWriteExt;
use tokio::net::lookup_host;
use tokio::time::timeout;
use tracing::info;

use crate::minecraft::{
    decode_pong_response, decode_status_response, encode_handshake, ping_request_packet,
    HandshakeInfo, PacketIo,
};

use super::service::MotdService;

pub struct UpstreamStatusSession {
    stream: tokio::net::TcpStream,
    packet_io: PacketIo,
    target_addr: std::net::SocketAddr,
    cached_status_json: Option<Arc<str>>,
    op_timeout: Duration,
}

impl UpstreamStatusSession {
    pub async fn connect(
        target_addr: &str,
        rewrite_addr: &str,
        handshake: &HandshakeInfo,
        timeout_limit: Duration,
        service: &MotdService,
        cached_status_json: Option<Arc<str>>,
        needs_status_json: bool,
        needs_ping: bool,
    ) -> anyhow::Result<Self> {
        let address = lookup_host(target_addr)
            .await
            .map_err(|error| anyhow::anyhow!("resolve target address {target_addr}: {error}"))?
            .next()
            .ok_or_else(|| anyhow::anyhow!("resolved no socket address for {target_addr}"))?;
        let mut stream = timeout(timeout_limit, tokio::net::TcpStream::connect(address))
            .await
            .with_context(|| format!("connect upstream status {address} timed out"))??;

        let mut rewritten = handshake.clone();
        rewritten.rewrite_addr(rewrite_addr).map_err(|e| anyhow::anyhow!(e))?;

        let mut probe = encode_handshake(&rewritten).map_err(anyhow::Error::from)?;
        probe.extend_from_slice(&[1, 0]);
        timeout(timeout_limit, stream.write_all(&probe))
            .await
            .with_context(|| format!("write upstream status probe to {address} timed out"))??;

        let needs_fetch = cached_status_json.is_none() || needs_status_json || needs_ping;
        if needs_fetch {
            let mut packet_io = PacketIo::new();
            let frame = timeout(timeout_limit, packet_io.read_frame(&mut stream, 64 * 1024))
                .await
                .with_context(|| {
                    format!("read upstream status response from {address} timed out")
                })??;
            let json = decode_status_response(&frame).map_err(anyhow::Error::from)?;
            let json = Arc::<str>::from(json);
            service.store_cached_status_arc(target_addr, rewrite_addr, Arc::clone(&json));

            return Ok(Self {
                stream,
                packet_io,
                target_addr: address,
                cached_status_json: Some(json),
                op_timeout: timeout_limit,
            });
        }

        Ok(Self {
            stream,
            packet_io: PacketIo::new(),
            target_addr: address,
            cached_status_json,
            op_timeout: timeout_limit,
        })
    }

    pub async fn read_status_json(&mut self) -> anyhow::Result<&str> {
        if self.cached_status_json.is_none() {
            let frame = timeout(
                self.op_timeout,
                self.packet_io.read_frame(&mut self.stream, 64 * 1024),
            )
            .await
            .with_context(|| {
                format!(
                    "read upstream status response from {} timed out",
                    self.target_addr
                )
            })??;
            let json = decode_status_response(&frame).map_err(anyhow::Error::from)?;
            self.cached_status_json = Some(Arc::<str>::from(json));
        }

        Ok(self.cached_status_json.as_deref().unwrap_or("{}"))
    }

    pub async fn ping(&mut self, client_payload: u64) -> anyhow::Result<(u64, u32)> {
        let started = Instant::now();
        let packet = ping_request_packet(client_payload).map_err(anyhow::Error::from)?;
        timeout(self.op_timeout, self.stream.write_all(&packet))
            .await
            .with_context(|| format!("write upstream ping to {} timed out", self.target_addr))??;

        let frame = timeout(
            self.op_timeout,
            self.packet_io.read_frame(&mut self.stream, 64 * 1024),
        )
        .await
        .with_context(|| format!("read upstream pong from {} timed out", self.target_addr))??;
        let payload = decode_pong_response(&frame).map_err(anyhow::Error::from)?;
        let measured_ms = started.elapsed().as_millis().min(u32::MAX as u128) as u32;

        info!(
            target_addr = %self.target_addr,
            upstream_payload = payload,
            measured_ms = measured_ms,
            "measured upstream status ping"
        );

        Ok((payload, measured_ms))
    }
}
