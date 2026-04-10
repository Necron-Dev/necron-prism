use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use dashmap::DashMap;

use necron_prism_minecraft::{HandshakeInfo, INTENT_LOGIN, INTENT_STATUS};
use crate::session::{ConnectionSession, PlayerState};

#[derive(Clone, Default)]
pub struct ConnectionRegistry {
    sessions: Arc<DashMap<String, ConnectionSession>>,
    online_count: Arc<AtomicI32>,
}

impl ConnectionRegistry {
    pub fn register(&self, session: ConnectionSession) -> usize {
        let connection_id = session.id.as_ref().expect("session must have connection_id");
        self.sessions.insert(connection_id.clone(), session);
        self.sessions.len()
    }

    pub fn update_handshake(&self, connection_id: &str, handshake: &HandshakeInfo) {
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

    pub fn update_login(&self, connection_id: &str, username: String, uuid: Option<valence_protocol::uuid::Uuid>) {
        self.update(connection_id, |session| {
            session.username = Some(username);
            session.uuid = uuid;
        });
    }

    pub fn update_outbound(&self, connection_id: &str, outbound_name: Arc<str>) {
        let was_proxying = self
            .sessions
            .get(connection_id)
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

    pub fn set_state(&self, connection_id: &str, state: PlayerState) {
        let was_proxying = self
            .sessions
            .get(connection_id)
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

    pub fn remove_connection(&self, connection_id: &str) -> usize {
        if let Some((_, session)) = self.sessions.remove(connection_id)
            && session.state == PlayerState::Proxying
        {
            self.online_count.fetch_sub(1, Ordering::Relaxed);
        }
        self.sessions.len()
    }

    pub fn active_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn with_session<R, F>(&self, connection_id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&ConnectionSession) -> R,
    {
        self.sessions.get(connection_id).map(|s| f(s.value()))
    }

    fn update<F>(&self, connection_id: &str, update: F)
    where
        F: FnOnce(&mut ConnectionSession),
    {
        if let Some(mut session) = self.sessions.get_mut(connection_id) {
            update(&mut session);
        }
    }
}