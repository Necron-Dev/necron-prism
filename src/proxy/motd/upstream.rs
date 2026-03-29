use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::info;

use crate::minecraft::{
    decode_pong_response, decode_status_response, encode_handshake, ping_request_packet, HandshakeInfo,
    PacketIo, ProtocolError,
};

use super::service::MotdService;

pub struct UpstreamStatusSession {
    stream: TcpStream,
    packet_io: PacketIo,
    target_addr: std::net::SocketAddr,
    cached_status_json: Option<Arc<str>>,
}

impl UpstreamStatusSession {
    pub fn connect(
        target_addr: &str,
        rewrite_addr: &str,
        handshake: &HandshakeInfo,
        timeout: Duration,
        service: &MotdService,
        cached_status_json: Option<Arc<str>>,
        needs_status_json: bool,
        needs_ping: bool,
    ) -> Result<Self, ProtocolError> {
        let address = resolve_target_addr(target_addr)?;
        let mut stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_read_timeout(Some(timeout))?;

        let mut rewritten = handshake.clone();
        rewritten
            .rewrite_addr(rewrite_addr)
            .map_err(ProtocolError::decode)?;

        let mut probe = encode_handshake(&rewritten)?;
        probe.extend_from_slice(&[1, 0]);
        stream.write_all(&probe)?;

        let needs_fetch = cached_status_json.is_none() || needs_status_json || needs_ping;
        if needs_fetch {
            let mut packet_io = PacketIo::new();
            let frame = packet_io.read_frame(&mut stream, 64 * 1024)?;
            let json = decode_status_response(&frame)?;
            let json = Arc::<str>::from(json);
            service.store_cached_status_arc(target_addr, rewrite_addr, Arc::clone(&json));

            return Ok(Self {
                stream,
                packet_io,
                target_addr: address,
                cached_status_json: Some(json),
            });
        }

        Ok(Self {
            stream,
            packet_io: PacketIo::new(),
            target_addr: address,
            cached_status_json,
        })
    }

    pub fn read_status_json(&mut self) -> Result<&str, ProtocolError> {
        if self.cached_status_json.is_none() {
            let frame = self.packet_io.read_frame(&mut self.stream, 64 * 1024)?;
            let json = decode_status_response(&frame)?;
            self.cached_status_json = Some(Arc::<str>::from(json));
        }

        Ok(self.cached_status_json.as_deref().unwrap_or("{}"))
    }
    pub fn ping(&mut self, client_payload: u64) -> Result<(u64, u32), ProtocolError> {
        let started = Instant::now();
        let packet = ping_request_packet(client_payload)?;
        self.stream.write_all(&packet)?;

        let frame = self.packet_io.read_frame(&mut self.stream, 64 * 1024)?;
        let payload = decode_pong_response(&frame)?;
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

fn resolve_target_addr(target_addr: &str) -> Result<std::net::SocketAddr, ProtocolError> {
    target_addr
        .to_socket_addrs()
        .map_err(|error| {
            ProtocolError::decode(format!("resolve target address {target_addr}: {error}"))
        })?
        .next()
        .ok_or_else(|| {
            ProtocolError::decode(format!("resolved no socket address for {target_addr}"))
        })
}
