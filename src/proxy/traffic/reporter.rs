use std::collections::HashMap;
use std::net::Shutdown;
use std::sync::{Arc, Mutex};

use tracing::warn;

use super::super::api::ApiService;
use super::super::config::ApiConfig;
use super::super::stats::ConnectionTraffic;
use super::counters::ConnectionCounters;

#[derive(Clone)]
pub struct TrafficReporter {
    api: Arc<ApiService>,
    sessions: Arc<Mutex<HashMap<u64, TrafficSession>>>,
    closers: Arc<Mutex<HashMap<Arc<str>, std::net::TcpStream>>>,
}

impl TrafficReporter {
    pub fn new(api: Arc<ApiService>, config: &ApiConfig) -> Self {
        let reporter = Self {
            api,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            closers: Arc::new(Mutex::new(HashMap::new())),
        };
        reporter.spawn_loop(config.traffic_interval);
        reporter
    }

    pub fn register(
        &self,
        connection_id: u64,
        external_connection_id: &str,
        counters: ConnectionCounters,
        closer: std::net::TcpStream,
    ) {
        self.register_internal(connection_id, external_connection_id, counters, closer);
    }

    pub fn finish(&self, connection_id: u64, totals: ConnectionTraffic) {
        self.finish_internal(connection_id, totals);
    }

    pub fn active_totals(&self) -> ConnectionTraffic {
        self.active_totals_internal()
    }
}

impl TrafficReporter {
    fn register_internal(
        &self,
        connection_id: u64,
        external_connection_id: &str,
        counters: ConnectionCounters,
        closer: std::net::TcpStream,
    ) {
        let external_connection_id: Arc<str> = external_connection_id.to_owned().into();
        let mut sessions = self.sessions.lock().expect("traffic reporter poisoned");
        sessions.insert(
            connection_id,
            TrafficSession {
                external_connection_id: Arc::clone(&external_connection_id),
                counters,
                last_sent_upload: 0,
                last_sent_download: 0,
            },
        );

        self.closers
            .lock()
            .expect("traffic reporter closers poisoned")
            .insert(external_connection_id, closer);
    }

    fn finish_internal(&self, connection_id: u64, totals: ConnectionTraffic) {
        let session = self
            .sessions
            .lock()
            .expect("traffic reporter poisoned")
            .remove(&connection_id);

        if let Some(session) = session {
            self.closers
                .lock()
                .expect("traffic reporter closers poisoned")
                .remove(session.external_connection_id.as_ref());

            if let Err(error) = self.api.closed(
                &session.external_connection_id,
                totals.upload_bytes,
                totals.download_bytes,
            ) {
                warn!(
                    error = %error,
                    cid = %session.external_connection_id,
                    connection_id,
                    "failed to report closed api event"
                );
            }
        }
    }

    fn active_totals_internal(&self) -> ConnectionTraffic {
        let sessions = self.sessions.lock().expect("traffic reporter poisoned");
        let mut totals = ConnectionTraffic::default();

        for session in sessions.values() {
            totals = totals.combined_with(ConnectionTraffic {
                upload_bytes: session.counters.upload(),
                download_bytes: session.counters.download(),
            });
        }

        totals
    }

    fn spawn_loop(&self, interval: std::time::Duration) {
        let api = Arc::clone(&self.api);
        let sessions = Arc::clone(&self.sessions);
        let closers = Arc::clone(&self.closers);

        std::thread::spawn(move || loop {
            std::thread::sleep(interval);

            let snapshot = {
                let mut sessions = sessions.lock().expect("traffic reporter poisoned");
                let mut snapshot = Vec::new();
                for session in sessions.values_mut() {
                    let upload = session.counters.upload();
                    let download = session.counters.download();
                    let delta_upload = upload.saturating_sub(session.last_sent_upload);
                    let delta_download = download.saturating_sub(session.last_sent_download);
                    if delta_upload == 0 && delta_download == 0 {
                        continue;
                    }

                    session.last_sent_upload = upload;
                    session.last_sent_download = download;
                    snapshot.push((
                        session.external_connection_id.clone(),
                        delta_upload,
                        delta_download,
                    ));
                }
                snapshot
            };

            for (cid, send_bytes, recv_bytes) in snapshot {
                match api.traffic_single(&cid, send_bytes, recv_bytes) {
                    Ok(connections_to_close) => {
                        if !connections_to_close.is_empty() {
                            close_connections(&closers, &connections_to_close);
                            warn!(cid = %cid, close_count = connections_to_close.len(), "traffic api requested connection close list");
                        }
                    }
                    Err(error) => {
                        warn!(error = %error, cid = %cid, "failed to report traffic api event")
                    }
                }
            }
        });
    }
}

fn close_connections(
    closers: &Arc<Mutex<HashMap<Arc<str>, std::net::TcpStream>>>,
    connections_to_close: &[String],
) {
    let mut registered = closers.lock().expect("traffic reporter closers poisoned");
    for close_id in connections_to_close {
        if let Some(stream) = registered.remove(close_id.as_str()) {
            let _ = stream.shutdown(Shutdown::Both);
            warn!(cid = %close_id, "closed connection requested by traffic api");
        }
    }
}

struct TrafficSession {
    external_connection_id: Arc<str>,
    counters: ConnectionCounters,
    last_sent_upload: u64,
    last_sent_download: u64,
}
