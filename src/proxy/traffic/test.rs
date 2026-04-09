use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use crate::proxy::api::ApiService;
use prism::config::{ApiConfig, ApiMode};
use prism::{ConnectionSession, ConnectionTraffic};

use super::TrafficReporter;

fn mock_config() -> ApiConfig {
    ApiConfig {
        mode: ApiMode::Mock,
        base_url: None,
        bearer_token: None,
        timeout_ms: 1000,
        traffic_interval_ms: 60000,
        mock_target_addr: "backend:25565".to_owned(),
        mock_rewrite_addr: Some("backend:25565".to_owned()),
        mock_connection_id_prefix: "mock".to_owned(),
        mock_kick_reason: None,
    }
}

fn mock_reporter() -> TrafficReporter {
    let config = mock_config();
    let mock_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let api = Arc::new(ApiService::new(&config, mock_counter).expect("mock api should build"));
    TrafficReporter::new(api, &config)
}

fn connected_stream() -> TcpStream {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let addr = listener
        .local_addr()
        .expect("listener should have local addr");
    let connector = thread::spawn(move || TcpStream::connect(addr).expect("client should connect"));
    let (stream, _) = listener
        .accept()
        .expect("listener should accept connection");
    let _ = connector.join().expect("connector thread should finish");
    stream
}

#[test]
fn mock_mode_keeps_active_traffic_totals() {
    let reporter = mock_reporter();
    let session = ConnectionSession::new(1, None);
    let closer = connected_stream();

    reporter.register(1, "mock-1", session.clone(), None, None, Some(closer));
    session.add_upload(128);
    session.add_download(256);

    let totals = reporter.active_totals();
    assert_eq!(totals.upload_bytes, 128);
    assert_eq!(totals.download_bytes, 256);

    reporter.shutdown();
}

#[test]
fn active_totals_empty_when_no_sessions() {
    let reporter = mock_reporter();
    let totals = reporter.active_totals();
    assert_eq!(totals.upload_bytes, 0);
    assert_eq!(totals.download_bytes, 0);
    reporter.shutdown();
}

#[test]
fn active_totals_sums_multiple_sessions() {
    let reporter = mock_reporter();
    let s1 = ConnectionSession::new(1, None);
    let s2 = ConnectionSession::new(2, None);

    reporter.register(1, "mock-1", s1.clone(), None, None, None);
    reporter.register(2, "mock-2", s2.clone(), None, None, None);

    s1.add_upload(100);
    s1.add_download(200);
    s2.add_upload(300);
    s2.add_download(400);

    let totals = reporter.active_totals();
    assert_eq!(totals.upload_bytes, 400);
    assert_eq!(totals.download_bytes, 600);
    assert_eq!(totals.total_bytes(), 1000);

    reporter.shutdown();
}

#[test]
fn finish_removes_session_from_active_totals() {
    let reporter = mock_reporter();
    let session = ConnectionSession::new(1, None);

    reporter.register(1, "mock-1", session.clone(), None, None, None);
    session.add_upload(500);
    session.add_download(600);

    let before = reporter.active_totals();
    assert_eq!(before.upload_bytes, 500);
    assert_eq!(before.download_bytes, 600);

    reporter.finish(1, ConnectionTraffic {
        upload_bytes: 500,
        download_bytes: 600,
    });

    thread::sleep(std::time::Duration::from_millis(50));

    let after = reporter.active_totals();
    assert_eq!(after.upload_bytes, 0);
    assert_eq!(after.download_bytes, 0);

    reporter.shutdown();
}

#[test]
fn finish_for_unknown_id_is_noop() {
    let reporter = mock_reporter();
    reporter.finish(999, ConnectionTraffic {
        upload_bytes: 0,
        download_bytes: 0,
    });
    reporter.shutdown();
}

#[test]
fn register_without_closer_works() {
    let reporter = mock_reporter();
    let session = ConnectionSession::new(1, None);
    reporter.register(1, "mock-1", session.clone(), None, None, None);
    session.add_upload(42);
    let totals = reporter.active_totals();
    assert_eq!(totals.upload_bytes, 42);
    reporter.shutdown();
}

#[test]
fn shared_session_updates_reflected_in_reporter() {
    let reporter = mock_reporter();
    let session = ConnectionSession::new(1, None);
    reporter.register(1, "mock-1", session.clone(), None, None, None);

    session.add_upload(100);
    assert_eq!(reporter.active_totals().upload_bytes, 100);

    session.add_upload(200);
    assert_eq!(reporter.active_totals().upload_bytes, 300);

    reporter.shutdown();
}

#[test]
fn register_with_player_info() {
    let reporter = mock_reporter();
    let session = ConnectionSession::new(1, None);
    reporter.register(
        1,
        "mock-1",
        session.clone(),
        Some(Arc::<str>::from("TestPlayer")),
        Some(Arc::<str>::from("550e8400-e29b-41d4-a716-446655440000")),
        None,
    );
    session.add_upload(1000);
    let totals = reporter.active_totals();
    assert_eq!(totals.upload_bytes, 1000);
    reporter.shutdown();
}
