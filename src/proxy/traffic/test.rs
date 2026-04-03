use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::proxy::api::ApiService;
use crate::proxy::config::{ApiConfig, ApiMode, MockApiConfig};

use super::{ConnectionCounters, TrafficReporter};

#[test]
fn mock_mode_keeps_active_traffic_totals() {
    let config = ApiConfig {
        mode: ApiMode::Mock,
        base_url: None,
        bearer_token: None,
        timeout: Duration::from_secs(1),
        traffic_interval: Duration::from_secs(60),
        mock: MockApiConfig {
            target_addr: "backend:25565".to_owned(),
            rewrite_addr: Some("backend:25565".to_owned()),
            connection_id_prefix: "mock".to_owned(),
            kick_reason: None,
        },
    };
    let api = Arc::new(ApiService::new(&config).expect("mock api should build"));
    let reporter = TrafficReporter::new(api, &config);
    let counters = ConnectionCounters::default();
    let closer = connected_stream();

    reporter.register(1, "mock-1", counters.clone(), Some(closer));
    counters.add_upload(128);
    counters.add_download(256);

    let totals = reporter.active_totals();
    assert_eq!(totals.upload_bytes, 128);
    assert_eq!(totals.download_bytes, 256);
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
