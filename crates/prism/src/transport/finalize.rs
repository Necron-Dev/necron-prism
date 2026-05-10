use std::time::Instant;

use tracing::{debug, info, trace, warn};

use crate::context::PrismContext;
use crate::hooks::PrismHooks;
use crate::session::{ConnectionKind, ConnectionReport, ConnectionSession};

use super::outcome::ConnectionOutcome;

pub(super) fn finalize_connection<H: PrismHooks>(
    ctx: &PrismContext<H>,
    session: &ConnectionSession,
    started_at: Instant,
    outcome: ConnectionOutcome,
) {
    let report = outcome.report().clone();
    ctx.hooks().on_connection_finished(session, &report);
    let _settled = ctx
        .runtime()
        .totals
        .record_finished_connection(report.connection_traffic);

    if let Some(cid) = session.connection_id() {
        let remaining = ctx.runtime().connections.remove_connection(&cid);
        trace!(connection_id = %cid, active_remaining = remaining, "[FINISH] removed connection from registry");
    }

    let tag = session.kind().tag();
    let elapsed_ms = started_at.elapsed().as_millis() as u64;
    let active_remaining = ctx.runtime().connections.active_count();

    if let Some(mode) = report.relay_mode {
        trace!(relay_mode = %mode, "[{tag}] relay completed");
    }

    match outcome {
        ConnectionOutcome::Completed(_) | ConnectionOutcome::Handled(_) => {
            log_closed(session.kind(), &report, elapsed_ms, active_remaining, tag);
        }
        ConnectionOutcome::Failed {
            error,
            expected_disconnect,
            ..
        } => {
            log_failed(
                session.kind(),
                &report,
                error,
                expected_disconnect,
                elapsed_ms,
                active_remaining,
                tag,
            );
        }
    }
}

fn log_closed(
    kind: ConnectionKind,
    report: &ConnectionReport,
    elapsed_ms: u64,
    active_remaining: usize,
    tag: &str,
) {
    let upload = report.connection_traffic.upload_bytes;
    let download = report.connection_traffic.download_bytes;
    let target = report.target_addr.as_ref().map(ToString::to_string);

    if kind == ConnectionKind::Motd {
        debug!(
            elapsed_ms,
            upload_bytes = upload,
            download_bytes = download,
            active_remaining,
            "[{tag}] connection closed"
        );
    } else {
        info!(
            elapsed_ms,
            upload_bytes = upload,
            download_bytes = download,
            active_remaining,
            target_addr = target.as_deref(),
            "[{tag}] connection closed"
        );
    }
}

fn log_failed(
    kind: ConnectionKind,
    report: &ConnectionReport,
    error: anyhow::Error,
    expected_disconnect: bool,
    elapsed_ms: u64,
    active_remaining: usize,
    tag: &str,
) {
    let upload = report.connection_traffic.upload_bytes;
    let download = report.connection_traffic.download_bytes;
    let target = report.target_addr.as_ref().map(ToString::to_string);

    if expected_disconnect {
        if kind == ConnectionKind::Motd {
            debug!(error = %error, elapsed_ms, upload_bytes = upload, download_bytes = download, active_remaining, "[{tag}] connection closed");
        } else {
            info!(error = %error, elapsed_ms, upload_bytes = upload, download_bytes = download, active_remaining, target_addr = target.as_deref(), "[{tag}] connection closed");
        }
    } else {
        warn!(error = %error, elapsed_ms, upload_bytes = upload, download_bytes = download, active_remaining, target_addr = target.as_deref(), "[{tag}] connection failed");
    }
}
