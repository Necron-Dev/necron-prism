use std::collections::BTreeMap;
use std::net::Shutdown;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use flurry::HashMap;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::{debug, info, trace, warn};

use crate::proxy::api::{ApiService, TrafficBody};
use prism::{ConnectionSession, ConnectionTraffic};

pub(super) struct PlayerTraffic {
    pub(super) cid: Arc<str>,
    pub(super) player_name: Option<Arc<str>>,
    pub(super) player_uuid: Option<Arc<str>>,
    pub(super) upload_bytes: u64,
    pub(super) download_bytes: u64,
    pub(super) delta_upload_bytes: u64,
    pub(super) delta_download_bytes: u64,
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
    let mut snapshot = Vec::with_capacity(sessions.len());
    let mut aggregate = ConnectionTraffic::default();
    let mut aggregate_delta = ConnectionTraffic::default();

    for (_, record) in sessions.iter(&guard) {
        let upload = record.session.upload();
        let download = record.session.download();
        let delta_upload = upload.saturating_sub(record.last_sent_upload.load(Ordering::Relaxed));
        let delta_download =
            download.saturating_sub(record.last_sent_download.load(Ordering::Relaxed));

        snapshot.push(PlayerTraffic {
            cid: record.connection_id.clone(),
            player_name: record.player_name.clone(),
            player_uuid: record.player_uuid.clone(),
            upload_bytes: upload,
            download_bytes: download,
            delta_upload_bytes: delta_upload,
            delta_download_bytes: delta_download,
        });
        aggregate = aggregate.combined_with(ConnectionTraffic {
            upload_bytes: upload,
            download_bytes: download,
        });
        aggregate_delta = aggregate_delta.combined_with(ConnectionTraffic {
            upload_bytes: delta_upload,
            download_bytes: delta_download,
        });
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
    pub(super) last_sent_upload: AtomicU64,
    pub(super) last_sent_download: AtomicU64,
}

pub(super) struct TrafficReporterState {
    pub(super) api: Arc<ApiService>,
    pub(super) sessions: Arc<HashMap<String, TrafficRecord>>,
    pub(super) closers: Arc<HashMap<Arc<str>, std::net::TcpStream>>,
    pub(super) cancel_token: CancellationToken,
}

pub(super) async fn run_loop(state: TrafficReporterState, interval: Duration) {
    let interval_secs = interval.as_secs_f64();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => report_once(&state, interval_secs).await,
            _ = state.cancel_token.cancelled() => {
                trace!("traffic reporter loop received shutdown signal, exiting");
                break;
            }
        }
    }
}

async fn report_once(state: &TrafficReporterState, interval_secs: f64) {
    let (snapshot, aggregate, aggregate_delta) = collect_traffic_snapshot(&state.sessions);
    if snapshot.is_empty() {
        return;
    }

    info!(
        upload_mbps = bytes_to_mbps(aggregate_delta.upload_bytes, interval_secs),
        download_mbps = bytes_to_mbps(aggregate_delta.download_bytes, interval_secs),
        total_upload_bytes = aggregate.upload_bytes,
        total_download_bytes = aggregate.download_bytes,
        active = snapshot.len(),
        "[TRAFFIC] report"
    );
    if tracing::enabled!(tracing::Level::DEBUG) {
        let players: Vec<String> = snapshot
            .iter()
            .map(|p| {
                let name = p.player_name.as_deref().unwrap_or("-");
                let uuid = p.player_uuid.as_deref().unwrap_or("-");
                format!(
                    "{}(uuid={},up={}B,down={}B)",
                    name, uuid, p.upload_bytes, p.download_bytes
                )
            })
            .collect();
        debug!(players = ?players, "[TRAFFIC] player detail");
    }

    // 单次批量请求上报全部连接:逐连接串行会让一轮耗时随在线数 × API 延迟线性放大。
    // 数值为累计值(服务端覆盖语义),失败时不更新 last_sent,下一轮自动补报。
    let entries: BTreeMap<String, TrafficBody> = snapshot
        .iter()
        .filter(|p| p.delta_upload_bytes > 0 || p.delta_download_bytes > 0)
        .map(|p| {
            (
                p.cid.to_string(),
                TrafficBody {
                    send_bytes: p.upload_bytes,
                    recv_bytes: p.download_bytes,
                },
            )
        })
        .collect();
    if entries.is_empty() {
        return;
    }

    match state.api.traffic_batch(&entries).await {
        Ok(connections_to_close) => {
            mark_reported(&state.sessions, &snapshot, &entries);
            if !connections_to_close.is_empty() {
                close_connections(&state.closers, &connections_to_close);
                warn!(
                    close_count = connections_to_close.len(),
                    "traffic api requested connection close list"
                );
            }
        }
        Err(error) => {
            warn!(error = %error, report_count = entries.len(), "failed to report traffic api event")
        }
    }
}

fn mark_reported(
    sessions: &Arc<HashMap<String, TrafficRecord>>,
    snapshot: &[PlayerTraffic],
    reported: &BTreeMap<String, TrafficBody>,
) {
    let guard = sessions.guard();
    for player in snapshot {
        if !reported.contains_key(player.cid.as_ref()) {
            continue;
        }
        if let Some(record) = sessions.get(player.cid.as_ref(), &guard) {
            record
                .last_sent_upload
                .store(player.upload_bytes, Ordering::Relaxed);
            record
                .last_sent_download
                .store(player.download_bytes, Ordering::Relaxed);
        }
    }
}
