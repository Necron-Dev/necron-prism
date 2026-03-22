use super::super::players::PlayerRegistry;
use super::super::stats::TrafficStats;

pub fn spawn_stats_logger(
    stats: TrafficStats,
    players: PlayerRegistry,
    interval: std::time::Duration,
) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            tracing::info!(
                active_connections = players.active_count(),
                total_connections = stats.total_connections(),
                total_upload_bytes = stats.total_upload_bytes(),
                total_download_bytes = stats.total_download_bytes(),
                total_bytes = stats.total_bytes(),
                interval_secs = interval.as_secs(),
                "traffic stats"
            );
        }
    });
}
