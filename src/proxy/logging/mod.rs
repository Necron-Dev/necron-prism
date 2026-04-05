use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::fs;
use std::io;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{reload, EnvFilter, Layer};

mod fmt;
pub use fmt::AnsiFormatter;

use super::config::{LogFormat, LogRotation, LoggingConfig};

pub type LogHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

pub fn init_tracing(config: &LoggingConfig) -> Result<(Vec<WorkerGuard>, LogHandle)> {
    if let Some(file_config) = &config.file {
        rotate_log_file(
            &file_config.path,
            file_config.mode,
            &file_config.archive_pattern,
        )?;
    }

    let mut guards = Vec::new();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.level.as_filter_directive()));

    let (filter, reload_handle) = reload::Layer::new(filter);

    let stdout_writer = if config.async_enabled {
        let (writer, guard) = tracing_appender::non_blocking(std::io::stdout());
        guards.push(guard);
        Some(writer)
    } else {
        None
    };

    let stdout_layer = match config.format {
        LogFormat::Pretty | LogFormat::Compact => {
            let layer = tracing_subscriber::fmt::Layer::default()
                .with_target(false)
                .with_file(false)
                .with_line_number(false)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_ansi(true)
                .with_span_events(FmtSpan::NONE)
                .event_format(AnsiFormatter::new());

            if let Some(writer) = stdout_writer {
                layer.with_writer(writer).boxed()
            } else {
                layer.with_writer(std::io::stdout).boxed()
            }
        }
        LogFormat::Json => {
            let layer = tracing_subscriber::fmt::Layer::default()
                .json()
                .with_target(false)
                .with_file(false)
                .with_line_number(false)
                .with_current_span(false)
                .with_span_list(false)
                .flatten_event(true);

            if let Some(writer) = stdout_writer {
                layer.with_writer(writer).boxed()
            } else {
                layer.with_writer(std::io::stdout).boxed()
            }
        }
    };

    let file_layer = if let Some(file_config) = &config.file {
        let directory = file_config
            .path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let file_name = file_config
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid log file path"))?;

        let file_appender = tracing_appender::rolling::never(directory, file_name);
        let (writer, guard) = tracing_appender::non_blocking(file_appender);
        guards.push(guard);

        Some(
            tracing_subscriber::fmt::Layer::default()
                .json()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_current_span(true)
                .with_span_list(true)
                .flatten_event(true)
                .with_ansi(false)
                .with_writer(writer),
        )
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .map_err(|error| anyhow!("failed to initialize tracing subscriber: {error}"))?;

    Ok((guards, reload_handle))
}

pub fn rotate_log_file(path: &Path, rotation: LogRotation, archive_pattern: &str) -> Result<()> {
    if !path.exists() || rotation == LogRotation::None {
        return Ok(());
    }

    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    if !directory.exists() {
        fs::create_dir_all(directory).context("failed to create log directory")?;
    }

    let date = Local::now().format("%Y-%m-%d").to_string();
    let archived_path = find_available_archive_path(directory, archive_pattern, &date);

    match rotation {
        LogRotation::None => unreachable!(),
        LogRotation::Rename => {
            fs::rename(path, &archived_path).with_context(|| {
                format!("failed to rename log file to {}", archived_path.display())
            })?;
        }
        LogRotation::Compress => {
            let input = fs::File::open(path).context("failed to open current log for rotation")?;
            let output =
                fs::File::create(&archived_path).context("failed to create archive log file")?;
            let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());

            let mut input_reader = io::BufReader::new(input);
            io::copy(&mut input_reader, &mut encoder).context("failed to compress log file")?;
            encoder
                .finish()
                .context("failed to finish gzip compression")?;

            fs::remove_file(path).context("failed to remove rotated log file")?;
        }
    }

    Ok(())
}

fn find_available_archive_path(directory: &Path, pattern: &str, date: &str) -> std::path::PathBuf {
    let mut index = 1;
    loop {
        let filename = pattern
            .replace("{date}", date)
            .replace("{index}", &index.to_string());
        let path = directory.join(filename);
        if !path.exists() {
            return path;
        }
        index += 1;
    }
}
