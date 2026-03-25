use crate::minecraft::{
    decode_status_request, status_response_packet, HandshakeInfo, PacketIo, ProtocolError,
    MAX_STATUS_PACKET_SIZE,
};
use crate::proxy::config::TransportConfig;
use crate::proxy::players::{PlayerRegistry, PlayerState};
use std::io::Write;
use std::net::TcpStream;

use tracing::info;

use super::cache::StatusCache;
use super::context::StatusContext;

#[derive(Clone, Default)]
pub struct MotdService {
    cache: StatusCache,
}

impl MotdService {
    pub fn serve(
        &self,
        packet_io: &mut PacketIo,
        client: &mut TcpStream,
        transport: &TransportConfig,
        handshake: &HandshakeInfo,
        players: &PlayerRegistry,
        connection_id: u64,
    ) -> Result<(), ProtocolError> {
        let status_request = packet_io.read_frame(client, MAX_STATUS_PACKET_SIZE)?;
        decode_status_request(&status_request)?;

        let context = StatusContext::new(transport, handshake, self);
        let mut upstream = context.open_upstream()?;

        let motd_json = context.build_json(players, upstream.as_mut())?;
        let status_response = status_response_packet(&motd_json)?;
        client.write_all(&status_response)?;

        let outcome = context.finish(packet_io, client, upstream.as_mut())?;
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
    ) -> Option<String> {
        self.cache.read(target_addr, rewrite_addr, ttl)
    }

    pub fn store_cached_status(&self, target_addr: &str, rewrite_addr: &str, json: &str) {
        self.cache.write(target_addr, rewrite_addr, json)
    }
}
