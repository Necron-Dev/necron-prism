use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use necron_prism_minecraft::{
    decode_ping_request, decode_status_response, ping_response_packet,
    HandshakeInfo, PacketIo, RuntimeAddress, MAX_STATUS_PACKET_SIZE,
};

use super::service::MotdService;

pub struct UpstreamStatusSession {
    stream: TcpStream,
    packet_io: PacketIo,
    status_json: Option<Arc<str>>,
}

impl UpstreamStatusSession {
    pub async fn connect(
        target_addr: RuntimeAddress,
        rewrite_addr: RuntimeAddress,
        client_handshake: &HandshakeInfo,
        status_request_wire: &[u8],
        timeout_duration: Duration,
        _service: &MotdService,
        read_json: bool,
    ) -> anyhow::Result<Self> {
        let mut stream = tokio::time::timeout(
            timeout_duration,
            TcpStream::connect(target_addr.as_str()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("upstream status connection timed out"))??;

        stream.set_nodelay(true)?;

        let mut handshake = client_handshake.clone();
        handshake.rewrite_addr(&rewrite_addr).map_err(|e| anyhow::anyhow!(e))?;

        let handshake_packet = necron_prism_minecraft::encode_handshake(&handshake)
            .map_err(anyhow::Error::from)?;

        let mut combined = Vec::with_capacity(handshake_packet.len() + status_request_wire.len());
        combined.extend_from_slice(&handshake_packet);
        combined.extend_from_slice(status_request_wire);
        stream.write_all(&combined).await?;

        let mut packet_io = PacketIo::new();
        let status_json = if read_json {
            let frame = packet_io.read_frame(&mut stream, MAX_STATUS_PACKET_SIZE).await?;
            let json = decode_status_response(&frame).map_err(anyhow::Error::from)?;
            Some(Arc::<str>::from(json))
        } else {
            None
        };

        Ok(Self {
            stream,
            packet_io,
            status_json,
        })
    }

    pub async fn read_status_json(&mut self) -> anyhow::Result<&str> {
        if self.status_json.is_none() {
            let frame = self
                .packet_io
                .read_frame(&mut self.stream, MAX_STATUS_PACKET_SIZE)
                .await?;
            let json = decode_status_response(&frame).map_err(anyhow::Error::from)?;
            self.status_json = Some(Arc::<str>::from(json));
        }

        Ok(self.status_json.as_deref().unwrap())
    }

    pub async fn ping(&mut self, client_payload: u64) -> anyhow::Result<(u64, Option<u32>)> {
        let start = Instant::now();

        let ping_request = ping_response_packet(client_payload).map_err(anyhow::Error::from)?;
        self.stream.write_all(&ping_request).await?;

        let pong_frame = self
            .packet_io
            .read_frame(&mut self.stream, MAX_STATUS_PACKET_SIZE)
            .await?;
        let payload = decode_ping_request(&pong_frame).map_err(anyhow::Error::from)?;

        let measured_ms = start.elapsed().as_millis() as u32;

        Ok((payload, Some(measured_ms)))
    }
}
