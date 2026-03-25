mod lifecycle;
mod state;

use std::thread;

use tracing::{info, warn};

use self::lifecycle::run_connection;
use self::state::AppState;
use super::config::ConfigLoader;
use super::inbound::{bind_listener, prepare_client_stream};
use super::logging::init_tracing;
use super::traffic::spawn_stats_logger;
use super::transport::ConnectionContext;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let state = AppState::new(ConfigLoader::load_default()?)?;
    let listener = bind_listener(&state.config.inbound)?;

    if let Some(interval) = state.config.stats_log_interval {
        spawn_stats_logger(
            state.connection_stats.clone(),
            state.connection_totals.clone(),
            state.players.clone(),
            (*state.traffic_reporter).clone(),
            interval,
        );
    }

    info!(
        listen_addr = %state.config.inbound.listen_addr,
        motd_mode = ?state.config.transport.motd.mode,
        api_mode = ?state.config.api.mode,
        mock_target_addr = %state.config.api.mock.target_addr,
        relay_mode = ?state.config.relay.mode,
        config_path = %state.config.source_path.display(),
        "proxy listening"
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => handle_incoming_connection(state.clone(), stream),
            Err(error) => warn!(error = %error, "accept failed"),
        }
    }

    Ok(())
}

fn handle_incoming_connection(state: AppState, stream: std::net::TcpStream) {
    if let Err(error) = prepare_client_stream(&stream, &state.config.inbound) {
        warn!(error = %error, "failed to apply inbound socket options");
    }

    let connection_id = state.connection_stats.connection_opened();
    let connection_ip = stream.peer_addr().ok();
    let active_connections = state.players.register_connection(connection_id);

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

    thread::spawn(move || run_connection(state, stream, context));
}
