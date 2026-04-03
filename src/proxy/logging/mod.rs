use anyhow::{anyhow, Result};
use std::sync::OnceLock;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::{format, FmtSpan};
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::EnvFilter;

use super::config::{LogFormat, LoggingConfig};

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

pub fn init_tracing(config: &LoggingConfig) -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.level.as_filter_directive()));

    match config.format {
        LogFormat::Pretty => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(build_writer(config))
            .with_target(false)
            .with_file(false)
            .with_line_number(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_span_events(FmtSpan::NONE)
            .event_format(
                format()
                    .compact()
                    .with_target(false)
                    .with_file(false)
                    .with_line_number(false)
                    .with_thread_ids(false)
                    .with_thread_names(false),
            )
            .try_init()
            .map_err(|error| anyhow!("failed to initialize tracing subscriber: {error}"))?,
        LogFormat::Compact => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(build_writer(config))
            .with_target(false)
            .with_file(false)
            .with_line_number(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_span_events(FmtSpan::NONE)
            .event_format(
                format()
                    .compact()
                    .without_time()
                    .with_target(false)
                    .with_file(false)
                    .with_line_number(false)
                    .with_thread_ids(false)
                    .with_thread_names(false),
            )
            .try_init()
            .map_err(|error| anyhow!("failed to initialize tracing subscriber: {error}"))?,
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_writer(build_writer(config))
            .with_target(false)
            .with_file(false)
            .with_line_number(false)
            .with_current_span(false)
            .with_span_list(false)
            .flatten_event(true)
            .try_init()
            .map_err(|error| anyhow!("failed to initialize tracing subscriber: {error}"))?,
    }

    Ok(())
}

fn build_writer(config: &LoggingConfig) -> BoxMakeWriter {
    if config.async_enabled {
        let (writer, guard) = tracing_appender::non_blocking(std::io::stdout());
        let _ = LOG_GUARD.set(guard);
        BoxMakeWriter::new(writer)
    } else {
        BoxMakeWriter::new(std::io::stdout)
    }
}
