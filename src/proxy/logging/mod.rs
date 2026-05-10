use acta::{
    AsyncWriterMode, ConsoleConfig, ConsoleWriter, FileLoggingConfig,
    LoggingConfig as ActaLoggingConfig, TracingGuard,
};
use prism::config::LoggingConfig;

pub use acta::ReloadHandle;

fn to_acta_config(config: &LoggingConfig) -> ActaLoggingConfig {
    ActaLoggingConfig {
        level: config.level.clone(),
        console: Some(ConsoleConfig {
            format: config.format.clone(),
            ansi: true,
            writer: if config.async_enabled {
                ConsoleWriter::AsyncStdout(AsyncWriterMode::Custom)
            } else {
                ConsoleWriter::Stdout
            },
            show_path: true,
            show_spans: true,
            time_format: Some("%H:%M:%S".to_string()),
        }),
        file: config.file.as_ref().map(|f| FileLoggingConfig {
            path: f.path.clone(),
            rotation: f.mode,
        }),
    }
}

pub fn init_tracing(config: &LoggingConfig) -> anyhow::Result<TracingGuard> {
    acta::init_tracing(&to_acta_config(config)).map_err(Into::into)
}

pub fn reload_log_filter(handle: &ReloadHandle, level: acta::LogLevel) -> anyhow::Result<()> {
    handle.set_level(level).map_err(Into::into)
}
