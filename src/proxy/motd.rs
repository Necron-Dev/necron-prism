use std::io::Write;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::info;

use crate::minecraft::{
    HandshakeInfo, MAX_STATUS_PACKET_SIZE, PacketIo, ProtocolError, decode_ping_request,
    decode_pong_response, decode_status_request, decode_status_response, encode_handshake,
    ping_request_packet, ping_response_packet, status_response_packet,
};

use super::config::{MotdFaviconMode, MotdMode, OutboundConfig, StatusPingMode, TransportConfig};
use super::motd_json::rewrite_json;
use super::players::{PlayerRegistry, PlayerState};
use super::stats::ConnectionTraffic;
use super::template;

#[derive(Clone, Default)]
pub struct MotdService {
    cache: Arc<Mutex<Option<CachedStatus>>>,
}

impl MotdService {
    pub fn serve(
        &self,
        packet_io: &mut PacketIo,
        client: &mut TcpStream,
        transport: &TransportConfig,
        selected_outbound: &OutboundConfig,
        handshake: &HandshakeInfo,
        handshake_wire_bytes: usize,
        players: &PlayerRegistry,
        connection_id: u64,
    ) -> Result<ConnectionTraffic, ProtocolError> {
        let status_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE)?;
        decode_status_request(&status_request)?;

        let mut upstream =
            UpstreamStatusSession::optional(transport, selected_outbound, handshake, self)?;

        let motd_json = self.build_motd_json(transport, handshake, players, upstream.as_mut())?;
        let status_response = status_response_packet(&motd_json)?;
        client.write_all(&status_response)?;

        let outcome =
            StatusExchange::new(packet_io, client, transport, selected_outbound, handshake)
                .finish(upstream.as_mut())?;

        players.set_state(
            connection_id,
            PlayerState::StatusServedLocally,
            Instant::now(),
        );

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

        Ok(ConnectionTraffic {
            upload_bytes: (handshake_wire_bytes
                + status_request.wire_len
                + outcome.ping_request_bytes) as u64,
            download_bytes: (status_response.len() + outcome.pong_bytes) as u64,
        })
    }

    fn build_motd_json(
        &self,
        transport: &TransportConfig,
        handshake: &HandshakeInfo,
        players: &PlayerRegistry,
        upstream: Option<&mut UpstreamStatusSession>,
    ) -> Result<String, ProtocolError> {
        let base_json = match transport.motd.mode {
            MotdMode::Local => template::render(
                transport.motd.local_json.as_deref().unwrap_or("{}"),
                players,
            ),
            MotdMode::Upstream => upstream
                .ok_or_else(|| ProtocolError::decode("missing upstream MOTD session"))?
                .read_status_json()?
                .to_owned(),
        };

        let favicon_source = if transport.motd.mode == MotdMode::Local
            && matches!(transport.motd.favicon, MotdFaviconMode::Passthrough)
        {
            self.cached_status_json()
        } else {
            None
        };

        Ok(rewrite_json(
            &base_json,
            transport.motd.protocol_mode,
            handshake.protocol_version,
            transport.motd.rewrite.as_ref(),
            &transport.motd.favicon,
            favicon_source.as_deref(),
        ))
    }

    fn cached_status_json(&self) -> Option<String> {
        self.cache
            .lock()
            .expect("motd cache poisoned")
            .as_ref()
            .map(|cached| cached.json.clone())
    }

    fn read_cached_status(
        &self,
        target_addr: &str,
        rewrite_addr: &str,
        ttl: Duration,
    ) -> Option<String> {
        let cache = self.cache.lock().expect("motd cache poisoned");
        let cached = cache.as_ref()?;
        if cached.target_addr == target_addr
            && cached.rewrite_addr == rewrite_addr
            && cached.cached_at.elapsed() <= ttl
        {
            return Some(cached.json.clone());
        }
        None
    }

    fn store_cached_status(&self, target_addr: &str, rewrite_addr: &str, json: &str) {
        let mut cache = self.cache.lock().expect("motd cache poisoned");
        *cache = Some(CachedStatus {
            target_addr: target_addr.to_string(),
            rewrite_addr: rewrite_addr.to_string(),
            json: json.to_string(),
            cached_at: Instant::now(),
        });
    }
}

struct StatusExchange<'a> {
    packet_io: &'a mut PacketIo,
    client: &'a mut TcpStream,
    transport: &'a TransportConfig,
    selected_outbound: &'a OutboundConfig,
    handshake: &'a HandshakeInfo,
}

impl<'a> StatusExchange<'a> {
    fn new(
        packet_io: &'a mut PacketIo,
        client: &'a mut TcpStream,
        transport: &'a TransportConfig,
        selected_outbound: &'a OutboundConfig,
        handshake: &'a HandshakeInfo,
    ) -> Self {
        Self {
            packet_io,
            client,
            transport,
            selected_outbound,
            handshake,
        }
    }

    fn finish(
        self,
        upstream: Option<&mut UpstreamStatusSession>,
    ) -> Result<StatusOutcome, ProtocolError> {
        match self.transport.motd.ping_mode {
            StatusPingMode::Disconnect => {
                self.client.shutdown(Shutdown::Both)?;
                Ok(StatusOutcome::default())
            }
            StatusPingMode::ZeroMs => self.respond_with_payload(0, 0, None),
            StatusPingMode::Passthrough => {
                let ping_request = self
                    .packet_io
                    .read_frame(self.client, MAX_STATUS_PACKET_SIZE)?;
                let payload = decode_ping_request(&ping_request)?;
                self.respond_with_payload(payload, ping_request.wire_len, None)
            }
            StatusPingMode::UpstreamTcp => {
                let ping_request = self
                    .packet_io
                    .read_frame(self.client, MAX_STATUS_PACKET_SIZE)?;
                let client_payload = decode_ping_request(&ping_request)?;
                let (payload, measured_ms) = match upstream {
                    Some(session) => session.ping(client_payload),
                    None => {
                        let mut session = UpstreamStatusSession::connect(
                            self.selected_outbound,
                            self.handshake,
                            self.transport.motd.upstream_ping_timeout,
                            None,
                            self.transport.motd.status_cache_ttl,
                        )?;
                        let _ = session.read_status_json()?;
                        session.ping(client_payload)
                    }
                }?;
                self.respond_with_payload(payload, ping_request.wire_len, Some(measured_ms))
            }
        }
    }

    fn respond_with_payload(
        self,
        payload: u64,
        ping_request_bytes: usize,
        upstream_ping_ms: Option<u32>,
    ) -> Result<StatusOutcome, ProtocolError> {
        let pong = ping_response_packet(payload)?;
        self.client.write_all(&pong)?;

        Ok(StatusOutcome {
            ping_request_bytes,
            pong_bytes: pong.len(),
            pong_payload: Some(payload),
            upstream_ping_ms,
        })
    }
}

#[derive(Default)]
struct StatusOutcome {
    ping_request_bytes: usize,
    pong_bytes: usize,
    pong_payload: Option<u64>,
    upstream_ping_ms: Option<u32>,
}

struct CachedStatus {
    target_addr: String,
    rewrite_addr: String,
    json: String,
    cached_at: Instant,
}

struct UpstreamStatusSession {
    stream: TcpStream,
    packet_io: PacketIo,
    target_addr: std::net::SocketAddr,
    target_addr_key: String,
    rewrite_addr_key: String,
    cached_status_json: Option<String>,
    service: Option<MotdService>,
    status_cache_ttl: Duration,
}

impl UpstreamStatusSession {
    fn optional(
        transport: &TransportConfig,
        selected_outbound: &OutboundConfig,
        handshake: &HandshakeInfo,
        service: &MotdService,
    ) -> Result<Option<Self>, ProtocolError> {
        let needs_upstream = transport.motd.mode == MotdMode::Upstream
            || transport.motd.ping_mode == StatusPingMode::UpstreamTcp
            || matches!(transport.motd.favicon, MotdFaviconMode::Passthrough)
                && transport.motd.local_json.is_some();

        if !needs_upstream {
            return Ok(None);
        }

        Self::connect(
            selected_outbound,
            handshake,
            transport.motd.upstream_ping_timeout,
            Some(service.clone()),
            transport.motd.status_cache_ttl,
        )
        .map(Some)
    }

    fn connect(
        selected_outbound: &OutboundConfig,
        handshake: &HandshakeInfo,
        timeout: Duration,
        service: Option<MotdService>,
        status_cache_ttl: Duration,
    ) -> Result<Self, ProtocolError> {
        let address = resolve_target_addr(&selected_outbound.target_addr)?;
        let mut stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_read_timeout(Some(timeout))?;

        let mut rewritten = handshake.clone();
        rewritten
            .rewrite_addr(&selected_outbound.rewrite_addr)
            .map_err(ProtocolError::decode)?;

        let mut probe = encode_handshake(&rewritten)?;
        probe.extend_from_slice(&[1, 0]);
        stream.write_all(&probe)?;

        Ok(Self {
            stream,
            packet_io: PacketIo::new(),
            target_addr: address,
            target_addr_key: selected_outbound.target_addr.clone(),
            rewrite_addr_key: selected_outbound.rewrite_addr.clone(),
            cached_status_json: None,
            service,
            status_cache_ttl,
        })
    }

    fn read_status_json(&mut self) -> Result<&str, ProtocolError> {
        if self.cached_status_json.is_none() {
            if let Some(service) = &self.service {
                if let Some(json) = service.read_cached_status(
                    &self.target_addr_key,
                    &self.rewrite_addr_key,
                    self.status_cache_ttl,
                ) {
                    self.cached_status_json = Some(json);
                }
            }

            if self.cached_status_json.is_none() {
                let frame = self.packet_io.read_frame(&mut self.stream, 64 * 1024)?;
                let json = decode_status_response(&frame)?;
                if let Some(service) = &self.service {
                    service.store_cached_status(
                        &self.target_addr_key,
                        &self.rewrite_addr_key,
                        &json,
                    );
                }
                self.cached_status_json = Some(json);
            }
        }

        Ok(self.cached_status_json.as_deref().unwrap_or("{}"))
    }

    fn ping(&mut self, client_payload: u64) -> Result<(u64, u32), ProtocolError> {
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
