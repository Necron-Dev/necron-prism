use crate::config::{RelayConfig, RelayMode};
use crate::session::ConnectionSession;
use std::net::SocketAddr;

#[test]
fn default_config_has_expected_values() {
    let config = crate::config::Config::default();
    assert_eq!(config.network.socket.listen_addr, "0.0.0.0:25565");
    assert_eq!(config.network.relay.mode, RelayMode::Async);
}

#[test]
fn relay_label_matrix() {
    let cases = [
        (RelayMode::Async, "async"),
        (RelayMode::IoUring, "io_uring"),
        (RelayMode::Splice, "splice"),
    ];

    for (mode, expected) in cases {
        let relay = RelayConfig { mode };
        assert_eq!(relay.label(), expected);
    }
}

#[test]
fn connection_session_keeps_identity_fields() {
    let peer_addr: SocketAddr = "127.0.0.1:25565".parse().unwrap();
    let session = ConnectionSession::new(Some(peer_addr));

    assert_eq!(session.connection_id(), None);
    assert_eq!(session.peer_addr, Some(peer_addr));

    session.record_player_identity("alex", "550e8400-e29b-41d4-a716-446655440000");
    let _entered = session.enter_stage("relay");
}
