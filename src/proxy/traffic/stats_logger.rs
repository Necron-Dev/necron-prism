use super::super::players::PlayerRegistry;
use super::super::stats::{ConnectionStats, ConnectionTotals, ConnectionTraffic};
use super::reporter::TrafficReporter;

pub fn spawn_stats_logger(
    connection_stats: ConnectionStats,
    connection_totals: ConnectionTotals,
    players: PlayerRegistry,
    traffic_reporter: TrafficReporter,
    interval: std::time::Duration,
) {
    std::thread::spawn(move || {
        let mut previous_combined = ConnectionTraffic::default();

        loop {
            std::thread::sleep(interval);
            let settled = connection_totals.settled_totals();
            let active = traffic_reporter.active_totals();
            let combined = settled.combined_with(active);
            let interval_upload_bytes = combined
                .upload_bytes
                .saturating_sub(previous_combined.upload_bytes);
            let interval_download_bytes = combined
                .download_bytes
                .saturating_sub(previous_combined.download_bytes);
            let interval_secs = interval.as_secs_f64();
            let settled_connection_upload_mb = megabytes(settled.upload_bytes);
            let settled_connection_download_mb = megabytes(settled.download_bytes);
            let active_connection_upload_mb = megabytes(active.upload_bytes);
            let active_connection_download_mb = megabytes(active.download_bytes);
            let total_connection_upload_mb = megabytes(combined.upload_bytes);
            let total_connection_download_mb = megabytes(combined.download_bytes);
            let total_connection_mb = megabytes(combined.total_bytes());
            let interval_upload_mb = megabytes(interval_upload_bytes);
            let interval_download_mb = megabytes(interval_download_bytes);
            let connection_upload_mbps = megabits_per_second(interval_upload_bytes, interval_secs);
            let connection_download_mbps =
                megabits_per_second(interval_download_bytes, interval_secs);
            previous_combined = combined;

            tracing::info!(
                active_connections = players.active_count(),
                total_connections = connection_stats.total_connections(),
                settled_connection_upload_mb,
                settled_connection_download_mb,
                active_connection_upload_mb,
                active_connection_download_mb,
                total_connection_upload_mb,
                total_connection_download_mb,
                total_connection_mb,
                interval_upload_mb,
                interval_download_mb,
                interval_secs = interval.as_secs(),
                connection_upload_mbps,
                connection_download_mbps,
                "traffic stats"
            );
        }
    });
}

fn megabytes(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000.0
}

fn megabits_per_second(bytes: u64, interval_secs: f64) -> f64 {
    if interval_secs <= 0.0 {
        return 0.0;
    }

    (bytes as f64 * 8.0) / 1_000_000.0 / interval_secs
}
