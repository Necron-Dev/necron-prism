use std::time::Instant;
use tracing::{debug, info_span, warn};

use super::Context;
use super::transport::{handle_client, ConnectionContext, ConnectionReport, HandledConnection};

pub async fn handle_connection(ctx: Context, stream: tokio::net::TcpStream, conn: ConnectionContext) {
    let span = info_span!("connection", connection_id = conn.id, peer_addr = ?conn.peer_addr);
    let _guard = span.enter();
    let started_at = Instant::now();

    match handle_client(stream, ctx.clone(), conn).await {
        Ok(report) => finish_success(&ctx, conn, started_at, report),
        Err(error) => {
            if let Some(report) = error.downcast_ref::<HandledConnection>().map(|h| &h.0) {
                finish_success(&ctx, conn, started_at, report.clone());
            } else {
                let remaining = ctx.core.players.remove_connection(conn.id);
                warn!(
                    error = %error,
                    elapsed_ms = started_at.elapsed().as_millis() as u64,
                    active_remaining = remaining,
                    "connection failed"
                );
            }
        }
    }
}

fn finish_success(ctx: &Context, conn: ConnectionContext, started_at: Instant, report: ConnectionReport) {
    let services = ctx.services();
    services.traffic.finish(conn.id, report.connection_traffic);
    
    let settled = ctx.core.totals.record_finished_connection(report.connection_traffic);
    let remaining = ctx.core.players.remove_connection(conn.id);

    if let Some(mode) = report.relay_mode {
        debug!(relay_mode = %mode, "relay completed");
    }

    let elapsed_ms = started_at.elapsed().as_millis() as u64;
    let upload_mb = report.connection_traffic.upload_bytes as f64 / 1_000_000.0;
    let download_mb = report.connection_traffic.download_bytes as f64 / 1_000_000.0;
    
    debug!(
        connection_id = conn.id,
        elapsed_ms,
        upload_mb,
        download_mb,
        total_connections = ctx.core.stats.total_connections(),
        settled_upload_mb = settled.upload_bytes as f64 / 1_000_000.0,
        settled_download_mb = settled.download_bytes as f64 / 1_000_000.0,
        active_remaining = remaining,
        "connection closed and settled"
    );
}