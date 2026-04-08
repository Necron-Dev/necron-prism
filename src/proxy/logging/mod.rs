use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::fs;
use std::io;
use std::io::Write as _;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{reload, EnvFilter, Layer};

mod fmt;
pub use fmt::AnsiFormatter;

use super::config::{LogFormat, LogRotation, LoggingConfig};

pub type LogHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;
const STDOUT_QUEUE_CAPACITY: usize = 1024;

pub struct LogGuards {
    _stdout: Option<AsyncStdoutGuard>,
    _workers: Vec<WorkerGuard>,
}

struct AsyncStdoutState {
    sender: Mutex<Option<mpsc::SyncSender<AsyncLogMessage>>>,
}

enum AsyncLogMessage {
    Data(Vec<u8>),
    Shutdown,
}

struct AsyncStdoutGuard {
    state: Arc<AsyncStdoutState>,
    join: Option<thread::JoinHandle<()>>,
}

#[derive(Clone)]
struct AsyncStdoutMakeWriter {
    state: Arc<AsyncStdoutState>,
}

struct AsyncStdoutEventWriter {
    state: Arc<AsyncStdoutState>,
    buffer: Vec<u8>,
}

impl AsyncStdoutMakeWriter {
    fn new() -> (Self, AsyncStdoutGuard) {
        let (sender, receiver) = mpsc::sync_channel::<AsyncLogMessage>(STDOUT_QUEUE_CAPACITY);
        let state = Arc::new(AsyncStdoutState {
            sender: Mutex::new(Some(sender)),
        });
        let worker_state = state.clone();
        let join = thread::spawn(move || {
            let mut stdout = std::io::stdout().lock();
            while let Ok(message) = receiver.recv() {
                match message {
                    AsyncLogMessage::Data(data) => {
                        let _ = stdout.write_all(&data);
                        let _ = stdout.flush();
                    }
                    AsyncLogMessage::Shutdown => break,
                }
            }
            drop(worker_state);
        });

        (
            Self {
                state: state.clone(),
            },
            AsyncStdoutGuard {
                state,
                join: Some(join),
            },
        )
    }

    fn enqueue(&self, data: Vec<u8>) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        let sender = self
            .state
            .sender
            .lock()
            .map_err(|_| io::Error::other("stdout writer mutex poisoned"))?;

        if let Some(sender) = sender.as_ref() {
            sender.send(AsyncLogMessage::Data(data)).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "async stdout worker closed",
                )
            })
        } else {
            Ok(())
        }
    }
}

impl Drop for AsyncStdoutGuard {
    fn drop(&mut self) {
        if let Ok(mut sender) = self.state.sender.lock()
            && let Some(sender) = sender.take()
        {
            let _ = sender.send(AsyncLogMessage::Shutdown);
        }

        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl<'a> MakeWriter<'a> for AsyncStdoutMakeWriter {
    type Writer = AsyncStdoutEventWriter;

    fn make_writer(&self) -> Self::Writer {
        AsyncStdoutEventWriter {
            state: self.state.clone(),
            buffer: Vec::with_capacity(256),
        }
    }
}

impl io::Write for AsyncStdoutEventWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let writer = AsyncStdoutMakeWriter {
            state: self.state.clone(),
        };
        writer.enqueue(std::mem::take(&mut self.buffer))
    }
}

impl Drop for AsyncStdoutEventWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

pub fn init_tracing(config: &LoggingConfig) -> Result<(LogGuards, LogHandle)> {
    if let Some(file_config) = &config.file {
        rotate_log_file(
            &file_config.path,
            file_config.mode,
            &file_config.archive_pattern,
        )?;
    }

    let mut worker_guards = Vec::new();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.level.as_filter_directive()));

    let (filter, reload_handle) = reload::Layer::new(filter);

    let (stdout_writer, stdout_guard) = if config.async_enabled {
        let (writer, guard) = AsyncStdoutMakeWriter::new();
        (Some(writer), Some(guard))
    } else {
        (None, None)
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

            if let Some(writer) = stdout_writer.clone() {
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

            if let Some(writer) = stdout_writer.clone() {
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
        worker_guards.push(guard);

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

    Ok((
        LogGuards {
            _stdout: stdout_guard,
            _workers: worker_guards,
        },
        reload_handle,
    ))
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
