use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::minecraft::{HandshakeInfo, INTENT_LOGIN, INTENT_STATUS};

#[derive(Clone, Default)]
pub struct PlayerRegistry {
    sessions: Arc<RwLock<HashMap<u64, PlayerSession>>>,
}

impl PlayerRegistry {
    pub fn register_connection(
        &self,
        connection_id: u64,
        peer_addr: Option<SocketAddr>,
        now: Instant,
    ) -> usize {
        let mut sessions = self.sessions.write().expect("player registry poisoned");
        sessions.insert(
            connection_id,
            PlayerSession {
                connection_id,
                peer_addr,
                protocol_version: None,
                requested_host: None,
                requested_port: None,
                next_state: None,
                username: None,
                selected_outbound: None,
                rewritten_host: None,
                rewritten_port: None,
                state: PlayerState::Accepted,
                connected_at: now,
                last_updated_at: now,
            },
        );
        sessions.len()
    }

    pub fn update_handshake(&self, connection_id: u64, handshake: &HandshakeInfo, now: Instant) {
        self.update(connection_id, now, |session| {
            session.protocol_version = Some(handshake.protocol_version);
            session.requested_host = Some(handshake.server_address.clone());
            session.requested_port = Some(handshake.server_port);
            session.next_state = Some(handshake.next_state);
            session.state = match handshake.next_state {
                INTENT_STATUS => PlayerState::Status,
                INTENT_LOGIN => PlayerState::Login,
                _ => PlayerState::Handshaking,
            };
        });
    }

    pub fn update_username(&self, connection_id: u64, username: String, now: Instant) {
        self.update(connection_id, now, move |session| {
            session.username = Some(username.clone());
        });
    }

    pub fn update_outbound(
        &self,
        connection_id: u64,
        outbound_name: &str,
        rewritten_addr: &str,
        now: Instant,
    ) {
        self.update(connection_id, now, |session| {
            session.selected_outbound = Some(outbound_name.to_string());
            if let Some((host, port)) = split_host_port(rewritten_addr) {
                session.rewritten_host = Some(host.to_string());
                session.rewritten_port = Some(port);
            } else {
                session.rewritten_host = Some(rewritten_addr.to_string());
                session.rewritten_port = None;
            }
            session.state = match session.next_state {
                Some(INTENT_STATUS) => PlayerState::StatusProxying,
                Some(INTENT_LOGIN) => PlayerState::LoginProxying,
                _ => PlayerState::Proxying,
            };
        });
    }

    pub fn set_state(&self, connection_id: u64, state: PlayerState, now: Instant) {
        self.update(connection_id, now, |session| {
            session.state = state;
        });
    }

    pub fn current_online_count(&self) -> i32 {
        let sessions = self.sessions.read().expect("player registry poisoned");
        sessions
            .values()
            .filter(|session| {
                matches!(
                    session.state,
                    PlayerState::StatusProxying
                        | PlayerState::LoginProxying
                        | PlayerState::Proxying
                )
            })
            .count() as i32
    }

    pub fn remove_connection(&self, connection_id: u64) -> usize {
        let mut sessions = self.sessions.write().expect("player registry poisoned");
        sessions.remove(&connection_id);
        sessions.len()
    }

    pub fn active_count(&self) -> usize {
        self.sessions
            .read()
            .expect("player registry poisoned")
            .len()
    }

    pub fn snapshot(&self, now: Instant) -> PlayerRegistrySnapshot {
        let sessions = self.sessions.read().expect("player registry poisoned");
        let players = sessions
            .values()
            .cloned()
            .map(|session| session.into_snapshot(now))
            .collect::<Vec<_>>();

        PlayerRegistrySnapshot {
            active_sessions: players.len(),
            players,
        }
    }

    fn update<F>(&self, connection_id: u64, now: Instant, mut update: F)
    where
        F: FnMut(&mut PlayerSession),
    {
        let mut sessions = self.sessions.write().expect("player registry poisoned");
        if let Some(session) = sessions.get_mut(&connection_id) {
            update(session);
            session.last_updated_at = now;
        }
    }
}

fn split_host_port(addr: &str) -> Option<(&str, u16)> {
    if let Some(stripped) = addr.strip_prefix('[') {
        let (host, port) = stripped.split_once(']')?;
        let port = port.strip_prefix(':')?.parse::<u16>().ok()?;
        return Some((host, port));
    }

    let (host, port) = addr.rsplit_once(':')?;
    Some((host, port.parse::<u16>().ok()?))
}

#[derive(Clone, Debug)]
struct PlayerSession {
    connection_id: u64,
    peer_addr: Option<SocketAddr>,
    protocol_version: Option<i32>,
    requested_host: Option<String>,
    requested_port: Option<u16>,
    next_state: Option<i32>,
    username: Option<String>,
    selected_outbound: Option<String>,
    rewritten_host: Option<String>,
    rewritten_port: Option<u16>,
    state: PlayerState,
    connected_at: Instant,
    last_updated_at: Instant,
}

impl PlayerSession {
    fn into_snapshot(self, now: Instant) -> PlayerSnapshot {
        PlayerSnapshot {
            connection_id: self.connection_id,
            peer_addr: self.peer_addr,
            protocol_version: self.protocol_version,
            requested_host: self.requested_host,
            requested_port: self.requested_port,
            next_state: self.next_state,
            username: self.username,
            selected_outbound: self.selected_outbound,
            rewritten_host: self.rewritten_host,
            rewritten_port: self.rewritten_port,
            state: self.state,
            connected_for_ms: now.duration_since(self.connected_at).as_millis() as u64,
            idle_for_ms: now.duration_since(self.last_updated_at).as_millis() as u64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerState {
    Accepted,
    Handshaking,
    Status,
    Login,
    StatusServedLocally,
    LoginRejectedLocally,
    StatusProxying,
    LoginProxying,
    Proxying,
}

#[derive(Clone, Debug)]
pub struct PlayerSnapshot {
    pub connection_id: u64,
    pub peer_addr: Option<SocketAddr>,
    pub protocol_version: Option<i32>,
    pub requested_host: Option<String>,
    pub requested_port: Option<u16>,
    pub next_state: Option<i32>,
    pub username: Option<String>,
    pub selected_outbound: Option<String>,
    pub rewritten_host: Option<String>,
    pub rewritten_port: Option<u16>,
    pub state: PlayerState,
    pub connected_for_ms: u64,
    pub idle_for_ms: u64,
}

#[derive(Clone, Debug)]
pub struct PlayerRegistrySnapshot {
    pub active_sessions: usize,
    pub players: Vec<PlayerSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tracks_active_sessions() {
        let registry = PlayerRegistry::default();
        let now = Instant::now();

        assert_eq!(registry.register_connection(1, None, now), 1);
        assert_eq!(registry.active_count(), 1);
        assert_eq!(registry.remove_connection(1), 0);
        assert_eq!(registry.active_count(), 0);
    }
}
