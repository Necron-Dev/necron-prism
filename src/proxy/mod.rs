pub mod api;
mod hooks;
pub mod routing;
pub mod traffic;

use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use self::hooks::NecronPrismHooks;
use self::traffic::TrafficReporter;
use crate::config::{ConfigLoader, canonicalize_runtime_config};
use acta::{AsyncMode, Format, Icons, JsonOptions, LevelLabels, Style, Writer, WriterTarget};
use anyhow::Result;
use prism::PrismContext;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
pub async fn run() -> Result<()> {
    let mut config = ConfigLoader::load_default()?;
    let logging = &config.prism.logging;
    let mut writers = vec![
        Writer::stdout()
            .with_stacktrace(tracing::level_filters::LevelFilter::OFF)
            .with_target(if logging.async_enabled {
                WriterTarget::AsyncStdout(AsyncMode::Custom { buffer_size: 4096 })
            } else {
                WriterTarget::Stdout
            })
            .with_format(match logging.format.clone() {
                Format::Compact(formatter) => Format::Compact(formatter.with_style(Style {
                    icons: Icons::NERD,
                    labels: LevelLabels::SHORT,
                    ..Style::default()
                })),
                format => format,
            }),
    ];
    if let Some(file) = &logging.file {
        writers.push(
            Writer::default()
                .with_stacktrace(tracing::level_filters::LevelFilter::OFF)
                .with_target(WriterTarget::File(file.clone()))
                .with_format(Format::Json(JsonOptions {
                    target: true,
                    file: true,
                    line_number: true,
                    current_span: true,
                    span_list: true,
                    flatten_event: true,
                })),
        );
    }
    let guard = acta::init(
        acta::Config::builder()
            .with_filter(EnvFilter::try_new(&logging.level)?)
            .with_writers(writers)
            .build(),
    )?;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "starting necron-prism proxy"
    );

    canonicalize_runtime_config(&mut config);

    let api = std::sync::Arc::new(crate::proxy::api::ApiService::new(
        &config.api,
        std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    )?);
    let traffic = TrafficReporter::new(api.clone(), &config.api);
    let hooks = NecronPrismHooks::new(
        api,
        traffic.clone(),
        config
            .api
            .entry_node_key
            .clone()
            .unwrap_or_else(|| "default".to_string()),
    );
    let ctx = PrismContext::new(config.prism, hooks);
    let _traffic_guard = traffic;

    tokio::select! {
        res = prism::inbound::run(ctx.clone()) => res?,
        _ = async {
            let path = Path::new(".reload");
            let mut last = fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(UNIX_EPOCH);

            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let now = fs::metadata(path)
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(UNIX_EPOCH);
                if now > last {
                    last = now;
                    info!("detected .reload file touch, reloading...");
                    let mut new_config = match ConfigLoader::load_default() {
                        Ok(config) => config,
                        Err(error) => {
                            warn!("reload failed: {error}");
                            continue;
                        }
                    };
                    canonicalize_runtime_config(&mut new_config);
                    let filter = match EnvFilter::try_new(&new_config.prism.logging.level) {
                        Ok(filter) => filter,
                        Err(error) => {
                            warn!("reload failed: {error}");
                            continue;
                        }
                    };
                    if let Err(error) = guard.set_filter(filter) {
                        warn!("reload failed: {error}");
                        continue;
                    }
                    ctx.update_config(new_config.prism);
                }
            }
        } => {},
        _ = async {
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
                _ = ctrl_c => {},
                _ = terminate => {},
            }
        } => info!("received shutdown signal, initiating graceful shutdown..."),
    }

    info!("flushing logs...");
    info!("necron-prism shutdown complete");
    drop(guard);
    Ok(())
}
