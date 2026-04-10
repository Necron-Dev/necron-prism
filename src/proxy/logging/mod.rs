mod fmt;

pub use fmt::rotate_log_file;
pub use fmt::AnsiFormatter;

use prism::config::{LogFormat, LoggingConfig};
use tracing_subscriber::layer::Layered;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;

pub type LogHandle = tracing_appender::non_blocking::WorkerGuard;

type FmtLayer = Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>;
type InnerSubscriber = Layered<FmtLayer, tracing_subscriber::Registry>;
pub type ReloadHandle = tracing_subscriber::reload::Handle<EnvFilter, InnerSubscriber>;

pub fn init_tracing(
    config: &LoggingConfig,
) -> anyhow::Result<(Option<LogHandle>, Option<std::path::PathBuf>, ReloadHandle)> {
    let filter = EnvFilter::new(config.level.as_filter_directive());

    let (filter, reload_handle) = tracing_subscriber::reload::Layer::new(filter);

    let fmt_layer: FmtLayer = match config.format {
        LogFormat::Pretty | LogFormat::Compact => {
            tracing_subscriber::fmt::Layer::default()
                .with_target(false)
                .with_file(false)
                .with_line_number(false)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_ansi(true)
                .with_span_events(FmtSpan::NONE)
                .event_format(AnsiFormatter::new())
                .with_writer(std::io::stdout)
                .boxed()
        }
        LogFormat::Json => {
            tracing_subscriber::fmt::Layer::default()
                .json()
                .with_target(false)
                .with_file(false)
                .with_line_number(false)
                .with_current_span(false)
                .with_span_list(false)
                .flatten_event(true)
                .with_writer(std::io::stdout)
                .boxed()
        }
    };

    let subscriber = tracing_subscriber::Registry::default()
        .with(fmt_layer)
        .with(filter);

    let guard = if let Some(file_config) = &config.file {
        let path = std::path::Path::new(&file_config.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let path = resolve_log_path(path);

        let file_appender = tracing_appender::rolling::never(
            path.parent().unwrap_or(std::path::Path::new(".")),
            path.file_name().unwrap_or_default(),
        );

        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .with_current_span(true)
            .with_span_list(true)
            .flatten_event(true)
            .with_ansi(false)
            .with_writer(non_blocking);

        let subscriber = subscriber.with(file_layer);
        tracing::subscriber::set_global_default(subscriber)?;

        Some((guard, Some(path)))
    } else {
        tracing::subscriber::set_global_default(subscriber)?;
        None
    };

    let (guard, resolved_path) = match guard {
        Some((g, p)) => (Some(g), p),
        None => (None, None),
    };

    Ok((guard, resolved_path, reload_handle))
}

pub fn reload_log_filter(handle: &ReloadHandle, directive: &str) -> anyhow::Result<()> {
    let filter = EnvFilter::new(directive);
    handle.modify(|f| *f = filter)?;
    Ok(())
}

fn resolve_log_path(path: &std::path::Path) -> std::path::PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    match std::fs::OpenOptions::new()
        
        .append(true)
        .open(path)
    {
        Ok(file) => {
            drop(file);
            path.to_path_buf()
        }
        Err(_) => {
            let pid = std::process::id();
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("latest");
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("log");
            let new_name = format!("{stem}-{pid}.{ext}");
            path.with_file_name(new_name)
        }
    }
}
