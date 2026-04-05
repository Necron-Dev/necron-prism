mod api;
mod config;
mod context;
mod inbound;
mod lifecycle;
mod logging;
mod motd;
mod network;
mod outbound;
mod players;
pub mod relay;
mod routing;
mod stats;
mod template;
pub mod traffic;
mod transport;

pub use context::Context;

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use socket2::SockRef;
use tracing::{info, warn};

use self::config::ConfigLoader;
use self::inbound::bind_listener;
use self::lifecycle::handle_connection;
use self::logging::{init_tracing, rotate_log_file, LogHandle};
use self::network::apply_sockref_options;
use self::transport::ConnectionContext;

static ACCEPTED: AtomicU64 = AtomicU64::new(0);

#[tokio::main]
pub async fn run() -> Result<()> {
    let config = ConfigLoader::load_default()?;
    let log_config = config.logging.clone();
    let (guards, log_handle) = init_tracing(&log_config)?;

    #[cfg(not(target_os = "linux"))]
    warn!("running on non-linux platform, high-performance features like splice are disabled");

    info!(version = env!("CARGO_PKG_VERSION"), "starting necron-prism proxy");

    let ctx = Context::new(config)?;
    let std_listener = bind_listener(&ctx.config()).await?;
    std_listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;

    tokio::spawn(watch_reload_file(ctx.clone(), log_handle.clone()));

    tokio::select! {
        res = accept_loop(listener, ctx) => res?,
        _ = shutdown_signal() => info!("received shutdown signal, initiating graceful shutdown..."),
    }

    info!("flushing logs and compressing active log file...");
    drop(guards);

    if let Some(file_config) = &log_config.file {
        if let Err(e) = rotate_log_file(&file_config.path, file_config.mode, &file_config.archive_pattern) {
            eprintln!("failed to rotate log file on shutdown: {e}");
        }
    }

    info!("necron-prism shutdown complete");
    Ok(())
}

async fn accept_loop(listener: tokio::net::TcpListener, ctx: Context) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let accepted = ACCEPTED.fetch_add(1, Ordering::Relaxed) + 1;
        
        let config = ctx.config();
        let connection_id = ctx.core.stats.connection_opened();
        let peer_addr = stream.peer_addr().ok();
        let conn = ConnectionContext { id: connection_id, peer_addr };

        if let Err(e) = apply_sockref_options(SockRef::from(&stream), &config) {
            warn!(error = %e, "failed to apply inbound socket options");
        }

        let ctx = ctx.clone();
        tokio::spawn(async move {
            let active = ctx.core.players.register_connection(connection_id);
            info!(
                connection_id,
                peer_addr = ?peer_addr,
                active,
                total_accepted = accepted,
                "handling inbound connection"
            );
            handle_connection(ctx, stream, conn).await;
            info!(connection_id, "connection closed");
        });
    }
}

async fn watch_reload_file(ctx: Context, log_handle: LogHandle) {
    let path = Path::new(".reload");
    let mut last = mtime(path);

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let now = mtime(path);
        if now > last {
            last = now;
            info!("detected .reload file touch, reloading...");
            if let Err(e) = ctx.reload(&log_handle) {
                warn!("reload failed: {e}");
            }
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

fn mtime(path: &Path) -> SystemTime {
    fs::metadata(path).and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH)
}
