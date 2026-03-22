use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use valence_protocol::uuid::Uuid;

use crate::minecraft::{HandshakeInfo, INTENT_LOGIN, INTENT_STATUS};
use crate::proxy::outbound::SelectedOutbound;

#[derive(Clone, Default)]
pub struct PlayerRegistry {
    sessions: Arc<RwLock<HashMap<u64, PlayerSession>>>,
}

impl PlayerRegistry {
    pub fn register_connection(
        &self,
        connection_id: u64,
        connection_ip: Option<SocketAddr>,
    ) -> usize {
        let now = Instant::now();

        let mut sessions = self.sessions.write().expect("player registry poisoned");
        sessions.insert(
            connection_id,
            PlayerSession {
                connection_id,
                connection_ip,
                protocol_version: None,
                next_state: None,
                username: None,
                uuid: None,
                selected_outbound: None,
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
            session.next_state = Some(handshake.next_state);
            session.state = match handshake.next_state {
                INTENT_STATUS => PlayerState::Status,
                INTENT_LOGIN => PlayerState::Login,
                _ => PlayerState::Handshaking,
            };
        });
    }

    pub fn update_username(&self, connection_id: u64, username: String) {
        self.update(connection_id, Instant::now(), |session| {
            session.username = Some(username);
        });
    }

    pub fn update_uuid(&self, connection_id: u64, uuid: Uuid) {
        self.update(connection_id, Instant::now(), |session| {
            session.uuid = Some(uuid);
        });
    }

    pub fn clear_uuid(&self, connection_id: u64) {
        self.update(connection_id, Instant::now(), |session| {
            session.uuid = None;
        });
    }

    pub fn update_outbound(&self, connection_id: u64, selected_outbound: SelectedOutbound) {
        self.update(connection_id, Instant::now(), move |session| {
            session.selected_outbound = Some(selected_outbound);
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

    fn update<F>(&self, connection_id: u64, now: Instant, update: F)
    where
        F: FnOnce(&mut PlayerSession),
    {
        let mut sessions = self.sessions.write().expect("player registry poisoned");
        if let Some(session) = sessions.get_mut(&connection_id) {
            update(session);
            session.last_updated_at = now;
        }
    }
}

#[derive(Clone, Debug)]
struct PlayerSession {
    connection_id: u64,
    connection_ip: Option<SocketAddr>,
    username: Option<String>,
    uuid: Option<Uuid>,
    selected_outbound: Option<SelectedOutbound>,
    protocol_version: Option<i32>,
    next_state: Option<i32>,
    state: PlayerState,
    connected_at: Instant,
    last_updated_at: Instant,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tracks_active_sessions() {
        let registry = PlayerRegistry::default();

        assert_eq!(registry.register_connection(1, None), 1);
        assert_eq!(registry.active_count(), 1);
        assert_eq!(registry.remove_connection(1), 0);
        assert_eq!(registry.active_count(), 0);
    }
}
