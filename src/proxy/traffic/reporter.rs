use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::Shutdown;
use std::sync::Arc;
use std::thread;
use tokio_util::sync::CancellationToken;

use tracing::{debug, warn};

use super::super::api::ApiService;
use super::super::config::ApiConfig;
use super::super::stats::{ConnectionSession, ConnectionTraffic};

#[derive(Clone)]
pub struct TrafficReporter {
    api: Arc<ApiService>,
    sessions: Arc<Mutex<HashMap<u64, TrafficRecord>>>,
    closers: Arc<Mutex<HashMap<Arc<str>, std::net::TcpStream>>>,
    cancel_token: CancellationToken,
}

impl TrafficReporter {
    pub fn new(api: Arc<ApiService>, config: &ApiConfig) -> Self {
        let cancel_token = CancellationToken::new();
        let reporter = Self {
            api,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            closers: Arc::new(Mutex::new(HashMap::new())),
            cancel_token,
        };
        reporter.spawn_loop(std::time::Duration::from_millis(config.traffic_interval_ms));
        reporter
    }

    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    pub fn register(
        &self,
        connection_id: u64,
        external_connection_id: &str,
        session: ConnectionSession,
        closer: Option<std::net::TcpStream>,
    ) {
        self.register_internal(connection_id, external_connection_id, session, closer);
    }

    pub fn finish(&self, connection_id: u64, totals: ConnectionTraffic) {
        let reporter = self.clone();
        spawn_background(async move {
            reporter.finish_internal(connection_id, totals).await;
        });
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
        session: ConnectionSession,
        closer: Option<std::net::TcpStream>,
    ) {
        let external_connection_id: Arc<str> = external_connection_id.to_owned().into();
        let mut sessions = self.sessions.lock();
        sessions.insert(
            connection_id,
            TrafficRecord {
                external_connection_id: Arc::clone(&external_connection_id),
                session,
                last_sent: ConnectionTraffic::default(),
            },
        );

        if let Some(closer) = closer {
            self.closers.lock().insert(external_connection_id, closer);
        }
    }

    async fn finish_internal(&self, connection_id: u64, totals: ConnectionTraffic) {
        let session = self
            .sessions
            .lock()
            .remove(&connection_id);

        if let Some(session) = session {
            self.closers
                .lock()
                .remove(session.external_connection_id.as_ref());

            if let Err(error) = self.api.closed(
                &session.external_connection_id,
                totals.upload_bytes,
                totals.download_bytes,
            ).await {
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
        let sessions = self.sessions.lock();
        let mut totals = ConnectionTraffic::default();

        for session in sessions.values() {
            totals = totals.combined_with(ConnectionTraffic {
                upload_bytes: session.session.upload(),
                download_bytes: session.session.download(),
            });
        }

        totals
    }

    fn spawn_loop(&self, interval: std::time::Duration) {
        let reporter = self.clone();
        let cancel_token = self.cancel_token.clone();
        spawn_background(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        let snapshot = {
                            let mut sessions = reporter.sessions.lock();
                            let mut snapshot = Vec::new();
                            for session in sessions.values_mut() {
                                let upload = session.session.upload();
                                let download = session.session.download();
                                let delta_upload = upload.saturating_sub(session.last_sent.upload_bytes);
                                let delta_download = download.saturating_sub(session.last_sent.download_bytes);
                                if delta_upload == 0 && delta_download == 0 {
                                    continue;
                                }

                                session.last_sent = ConnectionTraffic {
                                    upload_bytes: upload,
                                    download_bytes: download,
                                };
                                snapshot.push((
                                    session.external_connection_id.clone(),
                                    delta_upload,
                                    delta_download,
                                ));
                            }
                            snapshot
                        };

                        for (cid, send_bytes, recv_bytes) in snapshot {
                            match reporter.api.traffic_single(&cid, send_bytes, recv_bytes).await {
                                Ok(connections_to_close) => {
                                    if !connections_to_close.is_empty() {
                                        close_connections(&reporter.closers, &connections_to_close);
                                        warn!(cid = %cid, close_count = connections_to_close.len(), "traffic api requested connection close list");
                                    }
                                }
                                Err(error) => {
                                    warn!(error = %error, cid = %cid, "failed to report traffic api event")
                                }
                            }
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        debug!("traffic reporter loop received shutdown signal, exiting");
                        break;
                    }
                }
            }
        });
    }
}

fn spawn_background<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
        return;
    }

    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build background tokio runtime");
        runtime.block_on(future);
    });
}


fn close_connections(
    closers: &Arc<Mutex<HashMap<Arc<str>, std::net::TcpStream>>>,
    connections_to_close: &[String],
) {
    let mut registered = closers.lock();
    for close_id in connections_to_close {
        if let Some(stream) = registered.remove(close_id.as_str()) {
            let _ = stream.shutdown(Shutdown::Both);
            warn!(cid = %close_id, "closed connection requested by traffic api");
        }
    }
}

struct TrafficRecord {
    external_connection_id: Arc<str>,
    session: ConnectionSession,
    last_sent: ConnectionTraffic,
}
