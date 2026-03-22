use valence_protocol::uuid::Uuid;

#[derive(Clone, Debug)]
pub(super) struct PlayerSession {
    pub external_connection_id: Option<String>,
    pub username: Option<String>,
    pub uuid: Option<Uuid>,
    pub outbound_name: Option<String>,
    pub protocol_version: Option<i32>,
    pub next_state: Option<i32>,
    pub state: PlayerState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerState {
    Connected,
    Routing,
    Login,
    StatusServedLocally,
    LoginRejectedLocally,
    Proxying,
}
