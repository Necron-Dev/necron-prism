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
        Err(error) if handled_connection(&error).is_some() => {
            if let Some(report) = handled_connection(&error) {
                log_connection_success(&state, context, started_at, report.clone())
            }
        }
        Err(error) => log_connection_failure(&state, context, started_at, error),
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
    let total_upload = state.stats.add_upload(report.traffic.upload_bytes);
    let total_download = state.stats.add_download(report.traffic.download_bytes);
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

    state.traffic_reporter.finish(context.id, report.traffic);

    info!(
        connection_id = context.id,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        upload_bytes = report.traffic.upload_bytes,
        download_bytes = report.traffic.download_bytes,
        total_bytes = report.traffic.total_bytes(),
        total_upload_bytes = total_upload,
        total_download_bytes = total_download,
        total_connections = state.stats.total_connections(),
        active_connections = active_remaining,
        current_online_players = state.players.current_online_count(),
        observed_total_bytes = state.stats.total_bytes(),
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
