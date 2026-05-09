use std::net::Shutdown;
use std::sync::Arc;
use std::sync::Mutex;

use flurry::HashMap;
use rayon::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::{info, trace, warn};

use crate::config::ApiConfig;
use crate::proxy::api::ApiService;
use prism::{ConnectionSession, ConnectionTraffic};

#[derive(Clone)]
pub struct TrafficReporter {
    api: Arc<ApiService>,
    sessions: Arc<HashMap<String, TrafficRecord>>,
    closers: Arc<HashMap<Arc<str>, std::net::TcpStream>>,
    cancel_token: CancellationToken,
    background_handle: Arc<Mutex<Option<BackgroundHandle>>>,
}

impl TrafficReporter {
    pub fn new(api: Arc<ApiService>, config: &ApiConfig) -> Self {
        let cancel_token = CancellationToken::new();
        let mut reporter = Self {
            api,
            sessions: Arc::new(HashMap::new()),
            closers: Arc::new(HashMap::new()),
            cancel_token,
            background_handle: Arc::new(Mutex::new(None)),
        };
        reporter.spawn_loop(std::time::Duration::from_millis(config.traffic_interval_ms));
        reporter
    }

    pub fn shutdown(&self) {
        self.cancel_token.cancel();
        if let Ok(mut handle) = self.background_handle.lock()
            && let Some(h) = handle.take()
        {
            h.blocking_wait();
        }
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
        let log_player_name = player_name.clone();
        let log_player_uuid = player_uuid.clone();
        let guard = self.sessions.guard();
        self.sessions.insert(
            connection_id.to_string(),
            TrafficRecord {
                connection_id: Arc::clone(&cid),
                session,
                player_name,
                player_uuid,
                last_sent: ConnectionTraffic::default(),
            },
            &guard,
        );

        info!(
            cid = %connection_id,
            active = self.sessions.len(),
            player_name = log_player_name.as_deref().unwrap_or("-"),
            player_uuid = log_player_uuid.as_deref().unwrap_or("-"),
            "[TRAFFIC] registered connection"
        );

        if let Some(closer) = closer {
            let closers_guard = self.closers.guard();
            self.closers.insert(cid, closer, &closers_guard);
        }
    }

    pub fn finish(&self, connection_id: &str, totals: ConnectionTraffic) {
        info!(
            cid = %connection_id,
            active_before = self.sessions.len(),
            "[TRAFFIC] finish requested"
        );
        let guard = self.sessions.guard();
        let Some(record) = self.sessions.remove(connection_id, &guard) else {
            warn!(
                cid = %connection_id,
                active = self.sessions.len(),
                "[TRAFFIC] finish could not find active connection"
            );
            return;
        };
        let guard = self.closers.guard();
        self.closers.remove(record.connection_id.as_ref(), &guard);

        info!(
            cid = %record.connection_id,
            active_after = self.sessions.len(),
            "[TRAFFIC] removed active connection"
        );

        let api = Arc::clone(&self.api);
        let connection_id = record.connection_id.clone();
        let player_name = record.player_name.clone();
        let player_uuid = record.player_uuid.clone();

        spawn_background(async move {
            info!(
                cid = %connection_id,
                player_name = player_name.as_deref().unwrap_or("-"),
                player_uuid = player_uuid.as_deref().unwrap_or("-"),
                upload_bytes = totals.upload_bytes,
                download_bytes = totals.download_bytes,
                "[TRAFFIC] connection closed"
            );

            if let Err(error) = api
                .closed(&connection_id, totals.upload_bytes, totals.download_bytes)
                .await
            {
                warn!(
                    error = %error,
                    cid = %connection_id,
                    "failed to report closed api event"
                );
            }
        });
    }

    pub fn active_totals(&self) -> ConnectionTraffic {
        let guard = self.sessions.guard();
        let entries: Vec<_> = self.sessions.iter(&guard).collect();

        entries
            .par_iter()
            .map(|(_, record)| ConnectionTraffic {
                upload_bytes: record.session.upload(),
                download_bytes: record.session.download(),
            })
            .reduce(ConnectionTraffic::default, ConnectionTraffic::combined_with)
    }

    fn spawn_loop(&mut self, interval: std::time::Duration) {
        let reporter = Self {
            api: self.api.clone(),
            sessions: self.sessions.clone(),
            closers: self.closers.clone(),
            cancel_token: self.cancel_token.clone(),
            background_handle: Arc::new(Mutex::new(None)),
        };
        let cancel_token = self.cancel_token.clone();
        let interval_secs = interval.as_secs_f64();
        let handle = spawn_background(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        let (snapshot, aggregate, aggregate_delta) = collect_traffic_snapshot(&reporter);

                        if snapshot.is_empty() {
                            continue;
                        }

                        {
                            let players: Vec<String> = snapshot.par_iter().map(|p| {
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
                            if player.delta_upload_bytes == 0 && player.delta_download_bytes == 0 {
                                continue;
                            }
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
        if let Ok(mut h) = self.background_handle.lock() {
            *h = handle;
        }
    }
}

impl Drop for TrafficReporter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn collect_traffic_snapshot(
    reporter: &TrafficReporter,
) -> (Vec<PlayerTraffic>, ConnectionTraffic, ConnectionTraffic) {
    let guard = reporter.sessions.guard();
    let entries: Vec<_> = reporter.sessions.iter(&guard).collect();

    let results: Vec<(PlayerTraffic, ConnectionTraffic, ConnectionTraffic)> = entries
        .par_iter()
        .map(|(_, record)| {
            let upload = record.session.upload();
            let download = record.session.download();
            let delta_upload = upload.saturating_sub(record.last_sent.upload_bytes);
            let delta_download = download.saturating_sub(record.last_sent.download_bytes);

            let player_traffic = PlayerTraffic {
                cid: record.connection_id.clone(),
                player_name: record.player_name.clone(),
                player_uuid: record.player_uuid.clone(),
                upload_bytes: upload,
                download_bytes: download,
                delta_upload_bytes: delta_upload,
                delta_download_bytes: delta_download,
            };

            let traffic = ConnectionTraffic {
                upload_bytes: upload,
                download_bytes: download,
            };
            let delta = ConnectionTraffic {
                upload_bytes: delta_upload,
                download_bytes: delta_download,
            };

            (player_traffic, traffic, delta)
        })
        .collect();

    let mut snapshot = Vec::with_capacity(results.len());
    let mut aggregate = ConnectionTraffic::default();
    let mut aggregate_delta = ConnectionTraffic::default();

    for (player_traffic, traffic, delta) in results {
        snapshot.push(player_traffic);
        aggregate = aggregate.combined_with(traffic);
        aggregate_delta = aggregate_delta.combined_with(delta);
    }

    (snapshot, aggregate, aggregate_delta)
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

enum BackgroundHandle {
    Tokio(tokio::task::JoinHandle<()>),
    Thread(std::thread::JoinHandle<()>),
}

impl BackgroundHandle {
    fn blocking_wait(self) {
        match self {
            // Already inside a tokio runtime — can't block_on, just abort.
            // The cancel_token was already set, so the task will exit soon anyway.
            BackgroundHandle::Tokio(join_handle) => {
                join_handle.abort();
            }
            // Sync context — safe to block the thread until completion.
            BackgroundHandle::Thread(handle) => {
                let _ = handle.join();
            }
        }
    }
}

fn spawn_background<F>(future: F) -> Option<BackgroundHandle>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let span = tracing::info_span!("traffic");
        return Some(BackgroundHandle::Tokio(
            handle.spawn(future.instrument(span)),
        ));
    }

    Some(BackgroundHandle::Thread(std::thread::spawn(move || {
        match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(future),
            Err(error) => {
                tracing::error!(error = %error, "failed to build background tokio runtime")
            }
        }
    })))
}

fn close_connections(
    closers: &HashMap<Arc<str>, std::net::TcpStream>,
    connections_to_close: &[String],
) {
    let guard = closers.guard();
    for close_id in connections_to_close {
        if let Some(stream) = closers.remove(close_id.as_str(), &guard) {
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
