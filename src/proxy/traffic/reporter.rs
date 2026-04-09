use std::net::Shutdown;
use std::sync::Arc;
use std::thread;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::proxy::api::ApiService;
use prism::config::ApiConfig;
use prism::{ConnectionSession, ConnectionTraffic};

#[derive(Clone)]
pub struct TrafficReporter {
    api: Arc<ApiService>,
    sessions: Arc<DashMap<u64, TrafficRecord>>,
    closers: Arc<DashMap<Arc<str>, std::net::TcpStream>>,
    cancel_token: CancellationToken,
}

impl TrafficReporter {
    pub fn new(api: Arc<ApiService>, config: &ApiConfig) -> Self {
        let cancel_token = CancellationToken::new();
        let reporter = Self {
            api,
            sessions: Arc::new(DashMap::new()),
            closers: Arc::new(DashMap::new()),
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
        let external_connection_id: Arc<str> = external_connection_id.to_owned().into();
        self.sessions.insert(
            connection_id,
            TrafficRecord {
                external_connection_id: Arc::clone(&external_connection_id),
                session,
                last_sent: ConnectionTraffic::default(),
            },
        );

        if let Some(closer) = closer {
            self.closers.insert(external_connection_id, closer);
        }
    }

    pub fn finish(&self, connection_id: u64, totals: ConnectionTraffic) {
        let reporter = self.clone();
        spawn_background(async move {
            if let Some((_, record)) = reporter.sessions.remove(&connection_id) {
                reporter.closers.remove(record.external_connection_id.as_ref());

                info!(
                    cid = %record.external_connection_id,
                    connection_id,
                    upload_bytes = totals.upload_bytes,
                    download_bytes = totals.download_bytes,
                    "[TRAFFIC] connection closed"
                );

                if let Err(error) = reporter
                    .api
                    .closed(
                        &record.external_connection_id,
                        totals.upload_bytes,
                        totals.download_bytes,
                    )
                    .await
                {
                    warn!(
                        error = %error,
                        cid = %record.external_connection_id,
                        connection_id,
                        "failed to report closed api event"
                    );
                }
            }
        });
    }

    pub fn active_totals(&self) -> ConnectionTraffic {
        let mut totals = ConnectionTraffic::default();
        for entry in self.sessions.iter() {
            totals = totals.combined_with(ConnectionTraffic {
                upload_bytes: entry.session.upload(),
                download_bytes: entry.session.download(),
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
                        let mut snapshot = Vec::new();
                        for mut entry in reporter.sessions.iter_mut() {
                            let upload = entry.session.upload();
                            let download = entry.session.download();
                            let delta_upload = upload.saturating_sub(entry.last_sent.upload_bytes);
                            let delta_download = download.saturating_sub(entry.last_sent.download_bytes);
                            if delta_upload == 0 && delta_download == 0 {
                                continue;
                            }

                            entry.last_sent = ConnectionTraffic {
                                upload_bytes: upload,
                                download_bytes: download,
                            };
                            snapshot.push((
                                entry.external_connection_id.clone(),
                                delta_upload,
                                delta_download,
                            ));
                        }

                        for (cid, send_bytes, recv_bytes) in snapshot {
                            info!(
                                cid = %cid,
                                delta_upload_bytes = send_bytes,
                                delta_download_bytes = recv_bytes,
                                "[TRAFFIC] periodic report"
                            );

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
    closers: &DashMap<Arc<str>, std::net::TcpStream>,
    connections_to_close: &[String],
) {
    for close_id in connections_to_close {
        if let Some((_, stream)) = closers.remove(close_id.as_str()) {
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
