use std::net::Shutdown;
use std::sync::Arc;
use std::time::Duration;

use flurry::HashMap;
use rayon::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::{info, trace, warn};
use tracing::Instrument;

use crate::proxy::api::ApiService;
use prism::{ConnectionSession, ConnectionTraffic};

pub(super) struct PlayerTraffic {
    pub(super) cid: Arc<str>,
    pub(super) player_name: Option<Arc<str>>,
    pub(super) player_uuid: Option<Arc<str>>,
    pub(super) upload_bytes: u64,
    pub(super) download_bytes: u64,
}

pub(super) enum BackgroundHandle {
    Tokio(tokio::task::JoinHandle<()>),
    Thread(std::thread::JoinHandle<()>),
}

impl BackgroundHandle {
    pub(super) fn blocking_wait(self) {
        match self {
            BackgroundHandle::Tokio(join_handle) => {
                join_handle.abort();
            }
            BackgroundHandle::Thread(handle) => {
                let _ = handle.join();
            }
        }
    }
}

pub(super) fn spawn_background<F>(future: F) -> Option<BackgroundHandle>
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

pub(super) fn collect_traffic_snapshot(
    sessions: &Arc<HashMap<String, TrafficRecord>>,
) -> (Vec<PlayerTraffic>, ConnectionTraffic, ConnectionTraffic) {
    let guard = sessions.guard();
    let entries: Vec<_> = sessions.iter(&guard).collect();

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

pub(super) fn close_connections(
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

pub(super) fn bytes_to_mbps(bytes: u64, interval_secs: f64) -> f64 {
    if interval_secs <= 0.0 {
        return 0.0;
    }
    (bytes as f64 * 8.0) / (interval_secs * 1_000_000.0)
}

pub(super) struct TrafficRecord {
    pub(super) connection_id: Arc<str>,
    pub(super) session: ConnectionSession,
    pub(super) player_name: Option<Arc<str>>,
    pub(super) player_uuid: Option<Arc<str>>,
    pub(super) last_sent: ConnectionTraffic,
}

pub(super) struct TrafficReporterState {
    pub(super) api: Arc<ApiService>,
    pub(super) sessions: Arc<HashMap<String, TrafficRecord>>,
    pub(super) closers: Arc<HashMap<Arc<str>, std::net::TcpStream>>,
    pub(super) cancel_token: CancellationToken,
}

pub(super) fn run_loop(
    state: TrafficReporterState,
    interval: Duration,
) {
    let interval_secs = interval.as_secs_f64();
    spawn_background(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    let (snapshot, aggregate, aggregate_delta) = collect_traffic_snapshot(&state.sessions);

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
                        if player.upload_bytes == 0 && player.download_bytes == 0 {
                            continue;
                        }
                        match state.api.traffic_single(&player.cid, player.upload_bytes, player.download_bytes).await {
                            Ok(connections_to_close) => {
                                if !connections_to_close.is_empty() {
                                    close_connections(&state.closers, &connections_to_close);
                                    warn!(cid = %player.cid, close_count = connections_to_close.len(), "traffic api requested connection close list");
                                }
                            }
                            Err(error) => {
                                warn!(error = %error, cid = %player.cid, "failed to report traffic api event")
                            }
                        }
                    }
                }
                _ = state.cancel_token.cancelled() => {
                    trace!("traffic reporter loop received shutdown signal, exiting");
                    break;
                }
            }
        }
    });
}