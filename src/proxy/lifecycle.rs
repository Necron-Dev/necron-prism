use std::time::Instant;
use tracing::{debug, warn};

use super::Context;
use super::stats::{ConnectionReport, ConnectionSession};
use super::transport::{handle_client, HandledConnection};

pub async fn handle_connection(ctx: Context, stream: tokio::net::TcpStream, session: ConnectionSession) {
    let logging_conn = session.clone();
    let _guard = logging_conn.enter_stage("CONNECT/LIFECYCLE");
    let started_at = Instant::now();

    match handle_client(stream, ctx.clone(), session.clone()).await {
        Ok(report) => finish_success(&ctx, session, started_at, report),
        Err(error) => {
            if let Some(report) = error.downcast_ref::<HandledConnection>().map(|h| &h.0) {
                finish_success(&ctx, session, started_at, report.clone());
            } else {
                let remaining = ctx.core.players.remove_connection(session.id);
                warn!(
                    connection_id = session.id,
                    error = %error,
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    active_remaining = remaining,
                    "[CONNECT/LIFECYCLE] connection failed"
                );
            }
        }
    }
}

fn finish_success(ctx: &Context, session: ConnectionSession, started_at: Instant, report: ConnectionReport) {
    let services = ctx.services();
    services.traffic.finish(session.id, report.connection_traffic);
    
    let settled = ctx.core.totals.record_finished_connection(report.connection_traffic);
    let remaining = ctx.core.players.remove_connection(session.id);

    if let Some(mode) = report.relay_mode {
        debug!(relay_mode = %mode, "[CONNECT/LIFECYCLE] relay completed");
    }

    let elapsed_ms = started_at.elapsed().as_millis() as u64;
    let upload_mb = report.connection_traffic.upload_bytes as f64 / 1_000_000.0;
    let download_mb = report.connection_traffic.download_bytes as f64 / 1_000_000.0;
    
    debug!(
        elapsed_ms,
        upload_mb,
        download_mb,
        total_connections = ctx.core.stats.total_connections(),
        settled_upload_mb = settled.upload_bytes as f64 / 1_000_000.0,
        settled_download_mb = settled.download_bytes as f64 / 1_000_000.0,
        active_remaining = remaining,
        target_addr = report.target_addr.as_ref().map(ToString::to_string).as_deref(),
        rewrite_addr = report.rewrite_addr.as_ref().map(ToString::to_string).as_deref(),
        "[CONNECT/LIFECYCLE] connection closed and settled"
    );
}
