use std::sync::Arc;

use super::*;

#[test]
fn traffic_default_is_zero() {
    let traffic = ConnectionTraffic::default();
    assert_eq!(traffic.upload_bytes, 0);
    assert_eq!(traffic.download_bytes, 0);
    assert_eq!(traffic.total_bytes(), 0);
}

#[test]
fn traffic_combined_with_adds_both_directions() {
    let a = ConnectionTraffic {
        upload_bytes: 100,
        download_bytes: 200,
    };
    let b = ConnectionTraffic {
        upload_bytes: 300,
        download_bytes: 400,
    };
    let combined = a.combined_with(b);
    assert_eq!(combined.upload_bytes, 400);
    assert_eq!(combined.download_bytes, 600);
    assert_eq!(combined.total_bytes(), 1000);
}

#[test]
fn traffic_total_bytes_sums_upload_and_download() {
    let traffic = ConnectionTraffic {
        upload_bytes: 42,
        download_bytes: 58,
    };
    assert_eq!(traffic.total_bytes(), 100);
}

#[test]
fn session_starts_with_zero_traffic() {
    let session = ConnectionSession::new(None);
    assert_eq!(session.upload(), 0);
    assert_eq!(session.download(), 0);
    let traffic = session.connection_traffic();
    assert_eq!(traffic.upload_bytes, 0);
    assert_eq!(traffic.download_bytes, 0);
}

#[test]
fn session_add_upload_accumulates() {
    let session = ConnectionSession::new(None);
    session.add_upload(100);
    session.add_upload(200);
    assert_eq!(session.upload(), 300);
    assert_eq!(session.download(), 0);
}

#[test]
fn session_add_download_accumulates() {
    let session = ConnectionSession::new(None);
    session.add_download(50);
    session.add_download(150);
    assert_eq!(session.upload(), 0);
    assert_eq!(session.download(), 200);
}

#[test]
fn session_clones_share_traffic_counters() {
    let session = ConnectionSession::new(None);
    let clone = session.clone();
    session.add_upload(100);
    clone.add_download(200);
    assert_eq!(session.upload(), 100);
    assert_eq!(session.download(), 200);
    assert_eq!(clone.upload(), 100);
    assert_eq!(clone.download(), 200);
}

#[test]
fn session_connection_traffic_returns_snapshot() {
    let session = ConnectionSession::new(None);
    session.add_upload(42);
    session.add_download(58);
    let traffic = session.connection_traffic();
    assert_eq!(traffic.upload_bytes, 42);
    assert_eq!(traffic.download_bytes, 58);
    assert_eq!(traffic.total_bytes(), 100);
}

#[test]
fn session_concurrent_traffic_updates() {
    let session = Arc::new(ConnectionSession::new(None));
    let mut handles = Vec::new();
    for i in 0..4 {
        let s = Arc::clone(&session);
        handles.push(std::thread::spawn(move || {
            s.add_upload(100 * (i + 1));
            s.add_download(50 * (i + 1));
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(session.upload(), 100 + 200 + 300 + 400);
    assert_eq!(session.download(), 50 + 100 + 150 + 200);
}

#[test]
fn report_new_captures_all_fields() {
    let traffic = ConnectionTraffic {
        upload_bytes: 1024,
        download_bytes: 2048,
    };
    let report = ConnectionReport::new(traffic, Some(RelayMode::StandardCopy), None, None);
    assert_eq!(report.connection_traffic.upload_bytes, 1024);
    assert_eq!(report.connection_traffic.download_bytes, 2048);
    assert_eq!(report.relay_mode, Some(RelayMode::StandardCopy));
    assert!(report.target_addr.is_none());
    assert!(report.rewrite_addr.is_none());
}

#[test]
fn route_carries_connection_id() {
    let route = ConnectionRoute {
        target_addr: prism_minecraft::RuntimeAddress::parse("127.0.0.1:25565").unwrap(),
        rewrite_addr: None,
        connection_id: Some(Arc::<str>::from("mock-1")),
        player_name: None,
        player_uuid: None,
    };
    assert_eq!(route.connection_id.as_deref(), Some("mock-1"));
}

#[test]
fn route_carries_player_info() {
    let route = ConnectionRoute {
        target_addr: prism_minecraft::RuntimeAddress::parse("127.0.0.1:25565").unwrap(),
        rewrite_addr: None,
        connection_id: Some(Arc::<str>::from("mock-1")),
        player_name: Some(Arc::<str>::from("TestPlayer")),
        player_uuid: Some(Arc::<str>::from("550e8400-e29b-41d4-a716-446655440000")),
    };
    assert_eq!(route.player_name.as_deref(), Some("TestPlayer"));
    assert_eq!(
        route.player_uuid.as_deref(),
        Some("550e8400-e29b-41d4-a716-446655440000")
    );
}

#[test]
fn session_starts_with_unknown_kind() {
    let session = ConnectionSession::new(None);
    assert_eq!(session.kind(), ConnectionKind::Unknown);
}

#[test]
fn session_set_kind_motd() {
    let session = ConnectionSession::new(None);
    session.set_kind(ConnectionKind::Motd);
    assert_eq!(session.kind(), ConnectionKind::Motd);
}

#[test]
fn session_set_kind_proxy() {
    let session = ConnectionSession::new(None);
    session.set_kind(ConnectionKind::Proxy);
    assert_eq!(session.kind(), ConnectionKind::Proxy);
}

#[test]
fn session_kind_shared_across_clones() {
    let session = ConnectionSession::new(None);
    let clone = session.clone();
    session.set_kind(ConnectionKind::Motd);
    assert_eq!(clone.kind(), ConnectionKind::Motd);
}

#[test]
fn connection_kind_tag() {
    assert_eq!(ConnectionKind::Unknown.tag(), "CONNECT");
    assert_eq!(ConnectionKind::Motd.tag(), "CONNECT/MOTD");
    assert_eq!(ConnectionKind::Proxy.tag(), "CONNECT/LIFECYCLE");
}
