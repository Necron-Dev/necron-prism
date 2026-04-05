use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use valence_protocol::uuid::Uuid;

use crate::minecraft::{HandshakeInfo, INTENT_LOGIN, INTENT_STATUS};

use super::types::{PlayerSession, PlayerState};

#[derive(Clone, Default)]
pub struct PlayerRegistry {
    sessions: Arc<DashMap<u64, PlayerSession>>,
    online_count: Arc<AtomicI32>,
}

impl PlayerRegistry {
    pub fn register_connection(&self, connection_id: u64) -> usize {
        self.sessions.insert(
            connection_id,
            PlayerSession {
                external_connection_id: None,
                protocol_version: None,
                next_state: None,
                username: None,
                uuid: None,
                outbound_name: None,
                state: PlayerState::Connected,
            },
        );
        self.sessions.len()
    }

    pub fn update_handshake(&self, connection_id: u64, handshake: &HandshakeInfo) {
        self.update(connection_id, |session| {
            session.protocol_version = Some(handshake.protocol_version);
            session.next_state = Some(handshake.next_state);
            session.state = match handshake.next_state {
                INTENT_STATUS => PlayerState::Routing,
                INTENT_LOGIN => PlayerState::Login,
                _ => PlayerState::Routing,
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
        external_connection_id: Arc<str>,
    ) {
        self.update(connection_id, |session| {
            session.external_connection_id = Some(external_connection_id);
        });
    }

    pub fn with_external_connection_id<R, F>(&self, connection_id: u64, f: F) -> Option<R>
    where
        F: FnOnce(&str) -> R,
    {
        self.sessions
            .get(&connection_id)
            .and_then(|session| session.external_connection_id.as_deref().map(f))
    }

    pub fn update_outbound(&self, connection_id: u64, outbound_name: Arc<str>) {
        let was_proxying = self
            .sessions
            .get(&connection_id)
            .map(|s| s.state == PlayerState::Proxying)
            .unwrap_or(false);

        self.update(connection_id, |session| {
            session.outbound_name = Some(outbound_name);
            session.state = PlayerState::Proxying;
        });

        if !was_proxying {
            self.online_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn set_state(&self, connection_id: u64, state: PlayerState) {
        let was_proxying = self
            .sessions
            .get(&connection_id)
            .map(|s| s.state == PlayerState::Proxying)
            .unwrap_or(false);

        let will_be_proxying = state == PlayerState::Proxying;

        self.update(connection_id, |session| {
            session.state = state;
        });

        match (was_proxying, will_be_proxying) {
            (false, true) => {
                self.online_count.fetch_add(1, Ordering::Relaxed);
            }
            (true, false) => {
                self.online_count.fetch_sub(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    pub fn current_online_count(&self) -> i32 {
        self.online_count.load(Ordering::Relaxed)
    }

    pub fn remove_connection(&self, connection_id: u64) -> usize {
        if let Some((_, session)) = self.sessions.remove(&connection_id)
            && session.state == PlayerState::Proxying
        {
            self.online_count.fetch_sub(1, Ordering::Relaxed);
        }
        self.sessions.len()
    }

    pub fn active_count(&self) -> usize {
        self.sessions.len()
    }

    fn update<F>(&self, connection_id: u64, update: F)
    where
        F: FnOnce(&mut PlayerSession),
    {
        if let Some(mut session) = self.sessions.get_mut(&connection_id) {
            update(&mut session);
        }
    }
}
