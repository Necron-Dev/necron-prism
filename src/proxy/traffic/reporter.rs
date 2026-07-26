use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use flurry::HashMap;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::config::ApiConfig;
use crate::proxy::api::ApiService;
use prism::{ConnectionSession, ConnectionTraffic};

use super::background::{
    BackgroundHandle, TrafficRecord, TrafficReporterState, run_loop, spawn_background,
};

const CLOSED_REPORT_MAX_CONCURRENT: usize = 16;

#[derive(Clone)]
pub struct TrafficReporter {
    api: Arc<ApiService>,
    sessions: Arc<HashMap<String, TrafficRecord>>,
    closers: Arc<HashMap<Arc<str>, std::net::TcpStream>>,
    cancel_token: CancellationToken,
    background_handle: Arc<Mutex<Option<BackgroundHandle>>>,
    closed_limiter: Arc<Semaphore>,
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
            closed_limiter: Arc::new(Semaphore::new(CLOSED_REPORT_MAX_CONCURRENT)),
        };
        reporter.spawn_loop(Duration::from_millis(config.traffic_interval_ms));
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
                last_sent_upload: AtomicU64::new(0),
                last_sent_download: AtomicU64::new(0),
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
        let limiter = Arc::clone(&self.closed_limiter);
        let connection_id = record.connection_id.clone();
        let player_name = record.player_name.clone();
        let player_uuid = record.player_uuid.clone();

        spawn_background(async move {
            let Ok(_permit) = limiter.acquire_owned().await else {
                return;
            };
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
        self.sessions
            .iter(&guard)
            .map(|(_, record)| ConnectionTraffic {
                upload_bytes: record.session.upload(),
                download_bytes: record.session.download(),
            })
            .fold(ConnectionTraffic::default(), |acc, traffic| {
                acc.combined_with(traffic)
            })
    }

    fn spawn_loop(&mut self, interval: Duration) {
        let state = TrafficReporterState {
            api: self.api.clone(),
            sessions: self.sessions.clone(),
            closers: self.closers.clone(),
            cancel_token: self.cancel_token.clone(),
        };
        let handle = spawn_background(run_loop(state, interval));
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
