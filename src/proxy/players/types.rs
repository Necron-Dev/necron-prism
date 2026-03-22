use valence_protocol::uuid::Uuid;

#[derive(Clone, Debug)]
pub(super) struct PlayerSession {
    pub username: Option<String>,
    pub uuid: Option<Uuid>,
    pub outbound_name: Option<String>,
    pub protocol_version: Option<i32>,
    pub next_state: Option<i32>,
    pub state: PlayerState,
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
