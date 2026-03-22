use std::io::Write;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use tracing::info;

use crate::minecraft::{
    HandshakeInfo, MAX_STATUS_PACKET_SIZE, PacketIo, ProtocolError, decode_ping_request,
    decode_pong_response, decode_status_request, decode_status_response, encode_handshake,
    ping_request_packet, ping_response_packet, status_response_packet,
};

use super::config::{MotdFaviconMode, MotdMode, StatusPingMode, TransportConfig};
use super::motd_json::rewrite_json;
use super::outbound::SelectedOutbound;
use super::players::{PlayerRegistry, PlayerState};
use super::stats::ConnectionTraffic;
use super::template;

pub fn serve_motd(
    packet_io: &mut PacketIo,
    client: &mut TcpStream,
    transport: &TransportConfig,
    selected_outbound: &SelectedOutbound,
    handshake: &HandshakeInfo,
    handshake_wire_bytes: usize,
    players: &PlayerRegistry,
    connection_id: u64,
) -> Result<ConnectionTraffic, ProtocolError> {
    let status_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE)?;
    decode_status_request(&status_request)?;

    let mut upstream = if transport.motd.mode == MotdMode::Upstream
        || matches!(transport.motd.favicon, MotdFaviconMode::Passthrough)
            && transport.motd.local_json.is_some()
    {
        Some(UpstreamStatusSession::connect(
            selected_outbound,
            handshake,
            transport.motd.upstream_ping_timeout,
        )?)
    } else {
        None
    };

    let motd_json = build_motd_json(transport, handshake, players, upstream.as_mut())?;
    let status_response = status_response_packet(&motd_json)?;
    client.write_all(&status_response)?;

    let outcome = StatusExchange::new(
        packet_io,
        client,
        transport,
        selected_outbound,
        handshake,
        upstream.as_mut(),
    )
    .finish()?;

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
        upload_bytes: (handshake_wire_bytes + status_request.wire_len + outcome.ping_request_bytes)
            as u64,
        download_bytes: (status_response.len() + outcome.pong_bytes) as u64,
    })
}

struct StatusExchange<'a> {
    packet_io: &'a mut PacketIo,
    client: &'a mut TcpStream,
    transport: &'a TransportConfig,
    selected_outbound: &'a SelectedOutbound,
    handshake: &'a HandshakeInfo,
    upstream: Option<&'a mut UpstreamStatusSession>,
}

impl<'a> StatusExchange<'a> {
    fn new(
        packet_io: &'a mut PacketIo,
        client: &'a mut TcpStream,
        transport: &'a TransportConfig,
        selected_outbound: &'a SelectedOutbound,
        handshake: &'a HandshakeInfo,
        upstream: Option<&'a mut UpstreamStatusSession>,
    ) -> Self {
        Self {
            packet_io,
            client,
            transport,
            selected_outbound,
            handshake,
            upstream,
        }
    }

    fn finish(mut self) -> Result<StatusOutcome, ProtocolError> {
        match self.transport.motd.ping_mode {
            StatusPingMode::Disconnect => {
                self.client.shutdown(Shutdown::Both)?;
                Ok(StatusOutcome::default())
            }
            StatusPingMode::ZeroMs => self.respond_immediate(0),
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
                let (payload, measured_ms) = self.forward_ping_to_upstream(client_payload)?;
                self.respond_with_payload(payload, ping_request.wire_len, Some(measured_ms))
            }
        }
    }

    fn respond_immediate(self, payload: u64) -> Result<StatusOutcome, ProtocolError> {
        self.respond_with_payload(payload, 0, None)
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

    fn forward_ping_to_upstream(
        &mut self,
        client_payload: u64,
    ) -> Result<(u64, u32), ProtocolError> {
        match self.upstream.as_deref_mut() {
            Some(session) => session.ping(client_payload),
            None => {
                let mut session = UpstreamStatusSession::connect(
                    self.selected_outbound,
                    self.handshake,
                    self.transport.motd.upstream_ping_timeout,
                )?;
                let _ = session.read_status_json()?;
                session.ping(client_payload)
            }
        }
    }
}

#[derive(Default)]
struct StatusOutcome {
    ping_request_bytes: usize,
    pong_bytes: usize,
    pong_payload: Option<u64>,
    upstream_ping_ms: Option<u32>,
}

fn build_motd_json(
    transport: &TransportConfig,
    handshake: &HandshakeInfo,
    players: &PlayerRegistry,
    mut upstream: Option<&mut UpstreamStatusSession>,
) -> Result<String, ProtocolError> {
    let base_json = match transport.motd.mode {
        MotdMode::Local => template::render(
            transport.motd.local_json.as_deref().unwrap_or("{}"),
            players,
        ),
        MotdMode::Upstream => upstream
            .as_deref_mut()
            .ok_or_else(|| ProtocolError::decode("missing upstream MOTD session"))?
            .read_status_json()?,
    };

    let favicon_source = if transport.motd.mode == MotdMode::Local
        && matches!(transport.motd.favicon, MotdFaviconMode::Passthrough)
    {
        upstream
            .as_deref_mut()
            .and_then(|session| session.read_status_json().ok())
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

struct UpstreamStatusSession {
    stream: TcpStream,
    packet_io: PacketIo,
    target_addr: String,
    cached_status_json: Option<String>,
}

impl UpstreamStatusSession {
    fn connect(
        selected_outbound: &SelectedOutbound,
        handshake: &HandshakeInfo,
        timeout: Duration,
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
            target_addr: selected_outbound.target_addr.clone(),
            cached_status_json: None,
        })
    }

    fn read_status_json(&mut self) -> Result<String, ProtocolError> {
        if let Some(json) = &self.cached_status_json {
            return Ok(json.clone());
        }

        let frame = self.packet_io.read_frame(&mut self.stream, 64 * 1024)?;
        let json = decode_status_response(&frame)?;
        self.cached_status_json = Some(json.clone());
        Ok(json)
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
