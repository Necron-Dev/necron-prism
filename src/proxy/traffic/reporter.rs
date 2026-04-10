use std::net::Shutdown;
use std::sync::Arc;
use std::thread;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{info, trace, warn};
use tracing::Instrument;

use crate::proxy::api::ApiService;
use prism::config::ApiConfig;
use prism::{ConnectionSession, ConnectionTraffic};

#[derive(Clone)]
pub struct TrafficReporter {
    api: Arc<ApiService>,
    sessions: Arc<DashMap<String, TrafficRecord>>,
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
        connection_id: &str,
        session: ConnectionSession,
        player_name: Option<Arc<str>>,
        player_uuid: Option<Arc<str>>,
        closer: Option<std::net::TcpStream>,
    ) {
        let cid: Arc<str> = connection_id.to_owned().into();
        self.sessions.insert(
            connection_id.to_string(),
            TrafficRecord {
                connection_id: Arc::clone(&cid),
                session,
                player_name,
                player_uuid,
                last_sent: ConnectionTraffic::default(),
            },
        );

        if let Some(closer) = closer {
            self.closers.insert(cid, closer);
        }
    }

    pub fn finish(&self, connection_id: &str, totals: ConnectionTraffic) {
        let reporter = self.clone();
        let cid = connection_id.to_string();
        spawn_background(async move {
            if let Some((_, record)) = reporter.sessions.remove(&cid) {
                reporter.closers.remove(record.connection_id.as_ref());

                info!(
                    cid = %record.connection_id,
                    player_name = record.player_name.as_deref().unwrap_or("-"),
                    player_uuid = record.player_uuid.as_deref().unwrap_or("-"),
                    upload_bytes = totals.upload_bytes,
                    download_bytes = totals.download_bytes,
                    "[TRAFFIC] connection closed"
                );

                if let Err(error) = reporter
                    .api
                    .closed(
                        &record.connection_id,
                        totals.upload_bytes,
                        totals.download_bytes,
                    )
                    .await
                {
                    warn!(
                        error = %error,
                        cid = %record.connection_id,
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
        let interval_secs = interval.as_secs_f64();
        spawn_background(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        let mut snapshot = Vec::new();
                        let mut aggregate = ConnectionTraffic::default();
                        let mut aggregate_delta = ConnectionTraffic::default();
                        for mut entry in reporter.sessions.iter_mut() {
                            let upload = entry.session.upload();
                            let download = entry.session.download();
                            let delta_upload = upload.saturating_sub(entry.last_sent.upload_bytes);
                            let delta_download = download.saturating_sub(entry.last_sent.download_bytes);

                            aggregate = aggregate.combined_with(ConnectionTraffic {
                                upload_bytes: upload,
                                download_bytes: download,
                            });
                            aggregate_delta = aggregate_delta.combined_with(ConnectionTraffic {
                                upload_bytes: delta_upload,
                                download_bytes: delta_download,
                            });

                            entry.last_sent = ConnectionTraffic {
                                upload_bytes: upload,
                                download_bytes: download,
                            };
                            snapshot.push(PlayerTraffic {
                                cid: entry.connection_id.clone(),
                                player_name: entry.player_name.clone(),
                                player_uuid: entry.player_uuid.clone(),
                                upload_bytes: upload,
                                download_bytes: download,
                                delta_upload_bytes: delta_upload,
                                delta_download_bytes: delta_download,
                            });
                        }

                        if snapshot.is_empty() {
                            continue;
                        }

                        {
                            let players: Vec<String> = snapshot.iter().map(|p| {
                                let name = p.player_name.as_deref().unwrap_or("-");
                                let uuid = p.player_uuid.as_deref().unwrap_or("-");
                                format!("{}(uuid={},up={}B,down={}B)", name, uuid, p.upload_bytes, p.download_bytes)
                            }).collect();
                            info!(
                                upload_mbps = bytes_to_mbps(aggregate_delta.upload_bytes, interval_secs),
                                download_mbps = bytes_to_mbps(aggregate_delta.download_bytes, interval_secs),
                                total_upload_bytes = aggregate.upload_bytes,
                                total_download_bytes = aggregate.download_bytes,
                                active = snapshot.len(),
                                players = ?players,
                                "[TRAFFIC] report"
                            );
                        }

                        for player in &snapshot {
                            match reporter.api.traffic_single(&player.cid, player.delta_upload_bytes, player.delta_download_bytes).await {
                                Ok(connections_to_close) => {
                                    if !connections_to_close.is_empty() {
                                        close_connections(&reporter.closers, &connections_to_close);
                                        warn!(cid = %player.cid, close_count = connections_to_close.len(), "traffic api requested connection close list");
                                    }
                                }
                                Err(error) => {
                                    warn!(error = %error, cid = %player.cid, "failed to report traffic api event")
                                }
                            }
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        trace!("traffic reporter loop received shutdown signal, exiting");
                        break;
                    }
                }
            }
        });
    }
}

struct PlayerTraffic {
    cid: Arc<str>,
    player_name: Option<Arc<str>>,
    player_uuid: Option<Arc<str>>,
    upload_bytes: u64,
    download_bytes: u64,
    delta_upload_bytes: u64,
    delta_download_bytes: u64,
}

fn spawn_background<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let span = tracing::info_span!("traffic");
        handle.spawn(future.instrument(span));
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
    connection_id: Arc<str>,
    session: ConnectionSession,
    player_name: Option<Arc<str>>,
    player_uuid: Option<Arc<str>>,
    last_sent: ConnectionTraffic,
}

fn bytes_to_mbps(bytes: u64, interval_secs: f64) -> f64 {
    if interval_secs <= 0.0 {
        return 0.0;
    }
    (bytes as f64 * 8.0) / (interval_secs * 1_000_000.0)
}