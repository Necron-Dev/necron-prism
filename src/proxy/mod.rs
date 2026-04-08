mod api;
pub mod config;
mod context;
mod inbound;
mod lifecycle;
mod logging;
mod motd;
mod network;
mod outbound;
mod players;
mod routing;
mod stats;
mod template;
pub mod traffic;
pub(crate) mod transport;

pub use context::Context;
pub(crate) use self::stats::ConnectionSession;
pub use self::transport::relay::RelayMode;

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tracing::{info, warn};

use self::config::ConfigLoader;
use self::inbound::run as run_inbound;
use self::logging::{init_tracing, rotate_log_file, LogHandle};

#[tokio::main]
pub async fn run() -> Result<()> {
    let config = ConfigLoader::load_default()?;
    let log_config = config.logging.clone();
    let (guards, log_handle) = init_tracing(&log_config)?;

    info!(version = env!("CARGO_PKG_VERSION"), "starting necron-prism proxy");

    let ctx = Context::new(config)?;

    tokio::spawn(watch_reload_file(ctx.clone(), log_handle.clone()));

    tokio::select! {
        res = run_inbound(ctx) => res?,
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
