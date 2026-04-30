use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use anyhow::Result;
use flurry::HashMap;

use crate::session::{ConnectionSession, PlayerState};
use prism_minecraft::{HandshakeInfo, INTENT_LOGIN, INTENT_STATUS};

#[derive(Clone, Default)]
pub struct ConnectionRegistry {
    sessions: Arc<HashMap<String, ConnectionSession>>,
    online_count: Arc<AtomicI32>,
}

impl ConnectionRegistry {
    pub fn register(&self, session: ConnectionSession) -> Result<usize> {
        let connection_id = session
            .connection_id()
            .ok_or_else(|| anyhow::anyhow!("session must have connection_id"))?;
        let guard = self.sessions.guard();
        self.sessions.insert(connection_id, session, &guard);
        Ok(self.sessions.len())
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

    pub fn update_login(
        &self,
        connection_id: &str,
        username: String,
        uuid: Option<valence_protocol::uuid::Uuid>,
    ) {
        self.update(connection_id, |session| {
            session.username = Some(username);
            session.uuid = uuid;
        });
    }

    pub fn update_outbound(&self, connection_id: &str, outbound_name: Arc<str>) {
        let was_proxying = {
            let guard = self.sessions.guard();
            self.sessions
                .get(connection_id, &guard)
                .map(|s| s.state == PlayerState::Proxying)
                .unwrap_or(false)
        };

        self.update(connection_id, |session| {
            session.outbound_name = Some(outbound_name);
            session.state = PlayerState::Proxying;
        });

        if !was_proxying {
            self.online_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn set_state(&self, connection_id: &str, state: PlayerState) {
        let was_proxying = {
            let guard = self.sessions.guard();
            self.sessions
                .get(connection_id, &guard)
                .map(|s| s.state == PlayerState::Proxying)
                .unwrap_or(false)
        };

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
        let guard = self.sessions.guard();
        if let Some(session) = self.sessions.remove(connection_id, &guard)
            && session.state == PlayerState::Proxying
        {
            self.online_count.fetch_sub(1, Ordering::Relaxed);
        }
        self.sessions.len()
    }
    pub fn active_count(&self) -> usize {
        let _guard = self.sessions.guard();
        self.sessions.len()
    }
    pub fn with_session<R, F>(&self, connection_id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&ConnectionSession) -> R,
    {
        let guard = self.sessions.guard();
        self.sessions.get(connection_id, &guard).map(f)
    }
    fn update<F>(&self, connection_id: &str, update: F)
    where
        F: FnOnce(&mut ConnectionSession),
    {
        let guard = self.sessions.guard();
        if let Some(session) = self.sessions.get(connection_id, &guard) {
            let mut session = session.clone();
            update(&mut session);
            self.sessions
                .insert(connection_id.to_string(), session, &guard);
        }
    }
}
