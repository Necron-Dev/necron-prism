use std::io;
use std::time::Instant;

use tracing::{info, info_span, warn};

use super::state::AppState;
use crate::proxy::transport::{
    handle_client, ConnectionContext, ConnectionReport, HandledConnection,
};

pub fn run_connection(state: AppState, stream: std::net::TcpStream, context: ConnectionContext) {
    let span = info_span!("connection", connection_id = context.id, peer_addr = ?context.peer_addr);
    let _guard = span.enter();
    let started_at = Instant::now();

    match handle_client(
        stream,
        &state.config,
        &state.api,
        &state.motd,
        &state.traffic_reporter,
        &state.players,
        context,
        started_at,
    ) {
        Ok(report) => log_connection_success(&state, context, started_at, report),
        Err(error) => match handled_connection(&error) {
            Some(report) => log_connection_success(&state, context, started_at, report.clone()),
            None => log_connection_failure(&state, context, started_at, error),
        },
    }
}

fn handled_connection(error: &io::Error) -> Option<&ConnectionReport> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<HandledConnection>())
        .map(|handled| &handled.0)
}

fn log_connection_success(
    state: &AppState,
    context: ConnectionContext,
    started_at: Instant,
    report: ConnectionReport,
) {
    state
        .traffic_reporter
        .finish(context.id, report.connection_traffic);
    let settled_totals = state
        .connection_totals
        .record_finished_connection(report.connection_traffic);
    let active_remaining = state.players.remove_connection(context.id);

    if let Some(mode) = report.relay_mode {
        info!(relay_mode = %mode, "relay completed");
    }

    if !report.target_addr.is_empty() || !report.rewrite_addr.is_empty() {
        info!(
            target_addr = %report.target_addr,
            rewrite_addr = %report.rewrite_addr,
            "connection routed"
        );
    }

    info!(
        connection_id = context.id,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        connection_upload_mb = report.connection_traffic.upload_bytes as f64 / 1_000_000.0,
        connection_download_mb = report.connection_traffic.download_bytes as f64 / 1_000_000.0,
        connection_total_mb = report.connection_traffic.total_bytes() as f64 / 1_000_000.0,
        settled_connection_upload_mb = settled_totals.upload_bytes as f64 / 1_000_000.0,
        settled_connection_download_mb = settled_totals.download_bytes as f64 / 1_000_000.0,
        total_connections = state.connection_stats.total_connections(),
        active_connections = active_remaining,
        current_online_players = state.players.current_online_count(),
        observed_connection_total_mb = settled_totals.total_bytes() as f64 / 1_000_000.0,
        "connection finished"
    );
}

fn log_connection_failure(
    state: &AppState,
    context: ConnectionContext,
    started_at: Instant,
    error: io::Error,
) {
    let active_remaining = state.players.remove_connection(context.id);
    warn!(
        error = %error,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        active_connections = active_remaining,
        "connection failed"
    );
}
