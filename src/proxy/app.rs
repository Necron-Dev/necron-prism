mod lifecycle;
mod state;

use socket2::SockRef;
use tracing::{info, warn};
use crate::proxy::network::apply_sockref_options;
use self::state::AppState;
use super::config::ConfigLoader;
use super::inbound::{bind_listener};
use super::logging::init_tracing;
use super::traffic::spawn_stats_logger;
use super::transport::ConnectionContext;

#[tokio::main]
pub async fn run() -> anyhow::Result<()> {
    init_tracing();

    let state = AppState::new(ConfigLoader::load_default()?)?;
    let std_listener = bind_listener(&state.config.inbound)?;
    std_listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;

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

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state = state.clone();
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

                tokio::spawn(async move {
                    handle_incoming_connection(state, stream, context).await;
                });
            }
            Err(error) => warn!(error = %error, "accept failed"),
        }
    }
}

async fn handle_incoming_connection(
    state: AppState,
    stream: tokio::net::TcpStream,
    context: ConnectionContext,
) {
    if let Err(error) = apply_sockref_options(SockRef::from(&stream), &state.config.inbound.socket_options) {
        warn!(error = %error, "failed to apply inbound socket options");
    }

    lifecycle::run_connection(state, stream, context).await;
}
