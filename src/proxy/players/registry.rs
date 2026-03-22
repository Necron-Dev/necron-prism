use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use valence_protocol::uuid::Uuid;

use crate::minecraft::{HandshakeInfo, INTENT_LOGIN, INTENT_STATUS};

use super::types::{PlayerSession, PlayerState};

#[derive(Clone, Default)]
pub struct PlayerRegistry {
    sessions: Arc<RwLock<HashMap<u64, PlayerSession>>>,
}

impl PlayerRegistry {
    pub fn register_connection(&self, connection_id: u64) -> usize {
        let mut sessions = self.sessions.write().expect("player registry poisoned");
        sessions.insert(
            connection_id,
            PlayerSession {
                external_connection_id: None,
                protocol_version: None,
                next_state: None,
                username: None,
                uuid: None,
                outbound_name: None,
                state: PlayerState::Accepted,
            },
        );
        sessions.len()
    }

    pub fn update_handshake(&self, connection_id: u64, handshake: &HandshakeInfo) {
        self.update(connection_id, |session| {
            session.protocol_version = Some(handshake.protocol_version);
            session.next_state = Some(handshake.next_state);
            session.state = match handshake.next_state {
                INTENT_STATUS => PlayerState::Status,
                INTENT_LOGIN => PlayerState::Login,
                _ => PlayerState::Handshaking,
            };
        });
    }

    pub fn update_login(&self, connection_id: u64, username: String, uuid: Option<Uuid>) {
        self.update(connection_id, |session| {
            session.username = Some(username);
            session.uuid = uuid;
        });
    }

    pub fn update_external_connection_id(
        &self,
        connection_id: u64,
        external_connection_id: String,
    ) {
        self.update(connection_id, |session| {
            session.external_connection_id = Some(external_connection_id);
        });
    }

    pub fn external_connection_id(&self, connection_id: u64) -> Option<String> {
        self.sessions
            .read()
            .expect("player registry poisoned")
            .get(&connection_id)
            .and_then(|session| session.external_connection_id.clone())
    }

    pub fn update_outbound(&self, connection_id: u64, outbound_name: String) {
        self.update(connection_id, |session| {
            session.outbound_name = Some(outbound_name);
            session.state = match session.next_state {
                Some(INTENT_STATUS) => PlayerState::StatusProxying,
                Some(INTENT_LOGIN) => PlayerState::LoginProxying,
                _ => PlayerState::Proxying,
            };
        });
    }

    pub fn set_state(&self, connection_id: u64, state: PlayerState) {
        self.update(connection_id, |session| {
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

    fn update<F>(&self, connection_id: u64, update: F)
    where
        F: FnOnce(&mut PlayerSession),
    {
        let mut sessions = self.sessions.write().expect("player registry poisoned");
        if let Some(session) = sessions.get_mut(&connection_id) {
            update(session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tracks_active_sessions() {
        let registry = PlayerRegistry::default();

        assert_eq!(registry.register_connection(1), 1);
        assert_eq!(registry.active_count(), 1);
        assert_eq!(registry.remove_connection(1), 0);
        assert_eq!(registry.active_count(), 0);
    }
}
