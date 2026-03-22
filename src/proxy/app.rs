use std::sync::Arc;
use std::thread;
use std::time::Instant;

use tracing::{info, info_span, warn};

use super::api::ApiService;
use super::config::{Config, ConfigLoader};
use super::inbound::{bind_listener, prepare_client_stream};
use super::logging::init_tracing;
use super::players::PlayerRegistry;
use super::stats::TrafficStats;
use super::traffic::{TrafficReporter, spawn_stats_logger};
use super::transport::{ConnectionContext, ConnectionReport, handle_client};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config = Arc::new(ConfigLoader::load_default()?);
    let api = config
        .api
        .as_ref()
        .map(ApiService::new)
        .transpose()?
        .map(Arc::new);
    let traffic_reporter = api
        .as_ref()
        .zip(config.api.as_ref())
        .map(|(api, cfg)| Arc::new(TrafficReporter::new(Arc::clone(api), cfg)));
    let players = PlayerRegistry::default();
    let stats = TrafficStats::default();
    let listener = bind_listener(&config.inbound)?;

    if let Some(interval) = config.stats_log_interval {
        spawn_stats_logger(stats.clone(), players.clone(), interval);
    }

    info!(
        listen_addr = %config.inbound.listen_addr,
        fallback_target_addr = %config.outbounds.iter().find(|route| route.match_host.is_none()).map(|route| route.outbound.target_addr.as_str()).unwrap_or("<missing>"),
        fallback_rewrite_addr = %config.outbounds.iter().find(|route| route.match_host.is_none()).map(|route| route.outbound.rewrite_addr.as_str()).unwrap_or("<missing>"),
        motd_enabled = true,
        kick_enabled = config.transport.kick_json.is_some(),
        relay_mode = ?config.relay.mode,
        extra_outbounds = config.outbounds.len(),
        config_path = %config.source_path.display(),
        "proxy listening"
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                if let Err(error) = prepare_client_stream(&stream, &config.inbound) {
                    warn!(error = %error, "failed to apply inbound socket options");
                }

                let connection_id = stats.connection_opened();
                let connection_ip = stream.peer_addr().ok();
                let active_connections = players.register_connection(connection_id);

                info!(
                    connection_id,
                    peer_addr = ?connection_ip,
                    active_connections,
                    "accepted inbound connection"
                );

                let context = ConnectionContext {
                    id: connection_id,
                    peer_addr: connection_ip,
                };
                let config = Arc::clone(&config);
                let api = api.clone();
                let traffic_reporter = traffic_reporter.clone();
                let players = players.clone();
                let stats = stats.clone();
                thread::spawn(move || {
                    run_connection(
                        stream,
                        context,
                        config,
                        api,
                        traffic_reporter,
                        players,
                        stats,
                    )
                });
            }
            Err(error) => warn!(error = %error, "accept failed"),
        }
    }

    Ok(())
}

fn run_connection(
    stream: std::net::TcpStream,
    context: ConnectionContext,
    config: Arc<Config>,
    api: Option<Arc<ApiService>>,
    traffic_reporter: Option<Arc<TrafficReporter>>,
    players: PlayerRegistry,
    stats: TrafficStats,
) {
    let span = info_span!("connection", connection_id = context.id, peer_addr = ?context.peer_addr);
    let _guard = span.enter();
    let started_at = Instant::now();

    match handle_client(
        stream,
        &config,
        api.as_deref(),
        traffic_reporter.as_deref(),
        &players,
        context,
        started_at,
    ) {
        Ok(report) => log_connection_success(
            context,
            started_at,
            report,
            api.as_deref(),
            traffic_reporter.as_deref(),
            &players,
            &stats,
        ),
        Err(error) => {
            let active_remaining = players.remove_connection(context.id);
            warn!(
                error = %error,
                elapsed_ms = started_at.elapsed().as_millis() as u64,
                active_connections = active_remaining,
                "connection failed"
            );
        }
    }
}

fn log_connection_success(
    context: ConnectionContext,
    started_at: Instant,
    report: ConnectionReport,
    api: Option<&ApiService>,
    traffic_reporter: Option<&TrafficReporter>,
    players: &PlayerRegistry,
    stats: &TrafficStats,
) {
    let total_upload = stats.add_upload(report.traffic.upload_bytes);
    let total_download = stats.add_download(report.traffic.download_bytes);
    let active_remaining = players.remove_connection(context.id);

    if let Some(mode) = report.relay_mode {
        info!(relay_mode = %mode, "relay completed");
    }

    if let Some(outbound_name) = &report.outbound_name {
        info!(selected_outbound = %outbound_name, "outbound handled connection");
    }

    if let Some(reporter) = traffic_reporter {
        reporter.finish(context.id, report.traffic);
    } else if let (Some(api), Some(cid)) = (api, report.external_connection_id.as_ref()) {
        if let Err(error) = api.closed(
            cid,
            report.traffic.upload_bytes,
            report.traffic.download_bytes,
        ) {
            warn!(error = %error, connection_id = context.id, cid = %cid, "failed to report closed api event");
        }
    }

    info!(
        connection_id = context.id,
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        upload_bytes = report.traffic.upload_bytes,
        download_bytes = report.traffic.download_bytes,
        total_bytes = report.traffic.total_bytes(),
        total_upload_bytes = total_upload,
        total_download_bytes = total_download,
        total_connections = stats.total_connections(),
        active_connections = active_remaining,
        current_online_players = players.current_online_count(),
        observed_total_bytes = stats.total_bytes(),
        "connection finished"
    );
}
