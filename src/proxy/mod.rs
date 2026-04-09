pub mod api;
mod hooks;
mod logging;
pub mod routing;
pub mod traffic;

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tracing::{info, warn};

use prism::{PrismContext, Config, ConfigLoader};
use logging::{init_tracing, reload_log_filter, rotate_log_file, ReloadHandle};

use self::hooks::NecronPrismHooks;
use self::traffic::TrafficReporter;

type Context = PrismContext<NecronPrismHooks>;

#[tokio::main]
pub async fn run() -> Result<()> {
    let config = ConfigLoader::load_default()?;
    let log_config = config.logging.clone();
    let (guards, log_handle) = init_tracing(&log_config)?;

    info!(version = env!("CARGO_PKG_VERSION"), "starting necron-prism proxy");

    let (hooks, traffic) = build_hooks(&config)?;
    let ctx = Context::new(config, hooks);

    tokio::spawn(watch_reload_file(ctx.clone(), log_handle.clone()));
    let _traffic_guard = traffic;

    tokio::select! {
        res = prism::inbound::run(ctx) => res?,
        _ = shutdown_signal() => info!("received shutdown signal, initiating graceful shutdown..."),
    }

    info!("flushing logs and compressing active log file...");
    drop(guards);
    drop(_traffic_guard);

    if let Some(file_config) = &log_config.file {
        if let Err(e) = rotate_log_file(&file_config.path, file_config.mode, &file_config.archive_pattern) {
            eprintln!("failed to rotate log file on shutdown: {e}");
        }
    }

    info!("necron-prism shutdown complete");
    Ok(())
}

fn build_hooks(config: &Config) -> Result<(NecronPrismHooks, TrafficReporter)> {
    let api = std::sync::Arc::new(crate::proxy::api::ApiService::new(
        &config.api,
        std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    )?);
    let motd = std::sync::Arc::new(prism::motd::MotdService::new());
    let traffic = TrafficReporter::new(api.clone(), &config.api);

    Ok((NecronPrismHooks::new(api, motd, traffic.clone()), traffic))
}

async fn watch_reload_file(ctx: Context, log_handle: ReloadHandle) {
    let path = Path::new(".reload");
    let mut last = mtime(path);

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let now = mtime(path);
        if now > last {
            last = now;
            info!("detected .reload file touch, reloading...");
            if let Err(e) = reload_config(&ctx, &log_handle) {
                warn!("reload failed: {e}");
            }
        }
    }
}

fn reload_config(ctx: &Context, log_handle: &ReloadHandle) -> Result<()> {
    let new_config = ConfigLoader::load_default()?;
    new_config.validate()?;

    reload_log_filter(log_handle, new_config.logging.level.as_filter_directive())?;

    ctx.update_config(new_config);

    Ok(())
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
