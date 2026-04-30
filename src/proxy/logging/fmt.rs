use chrono::Local;
use owo_colors::{CssColors, Style};
use std::fmt;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::FormattedFields;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent};
use tracing_subscriber::registry::LookupSpan;

use prism::config::LogRotation;

#[allow(dead_code)]
pub struct AnsiFormatter;

#[allow(dead_code)]
const PATH_COLUMN_WIDTH: usize = 28;

#[allow(dead_code)]
mod theme {
    use owo_colors::Style;
    pub const BLUE: Style = Style::new().truecolor(91, 206, 250);
    pub const PINK: Style = Style::new().truecolor(245, 169, 184);
    pub const WHITE: Style = Style::new().truecolor(255, 255, 255);
}

#[allow(dead_code)]
impl AnsiFormatter {
    pub const fn new() -> Self {
        Self
    }

    fn format_path(file: &str, line: u32) -> String {
        let normalized = file.replace('\\', "/");
        let stripped = normalized
            .find("src/")
            .map(|i| &normalized[i + 4..])
            .unwrap_or(&normalized);

        Self::smart_truncate(stripped, line, PATH_COLUMN_WIDTH)
    }

    fn smart_truncate(path: &str, line: u32, max_width: usize) -> String {
        let full = format!("{}:{}", path, line);
        if full.len() <= max_width {
            return format!("{:>width$}", full, width = max_width);
        }

        if let Some(last_slash) = path.rfind('/') {
            let file_part = format!("{}:{}", &path[last_slash + 1..], line);
            if file_part.len() + 2 <= max_width {
                let dir_part = &path[..last_slash];
                let remaining = max_width - file_part.len() - 1;
                let dir_start = dir_part.len().saturating_sub(remaining);
                let clean_dir = dir_part[dir_start..]
                    .find('/')
                    .map(|i| &dir_part[dir_start + i + 1..])
                    .unwrap_or(&dir_part[dir_start..]);

                return format!("{}/{}", clean_dir, file_part);
            }
        }

        format!("…{}", &full[full.len().saturating_sub(max_width - 1)..])
    }
}

impl<S, N> FormatEvent<S, N> for AnsiFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> tracing_subscriber::fmt::FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        write!(writer, "{}", theme::BLUE.style("["))?;
        write!(
            writer,
            "{}",
            theme::WHITE.style(Local::now().format("%H:%M:%S").to_string())
        )?;
        write!(writer, "{}", theme::BLUE.style("] "))?;

        let level = event.metadata().level();
        let badge = match *level {
            Level::ERROR => Style::new()
                .on_color(CssColors::DarkRed)
                .white()
                .bold()
                .style(" ERROR "),
            Level::WARN => Style::new()
                .on_color(CssColors::Yellow)
                .black()
                .bold()
                .style("  WARN "),
            Level::INFO => Style::new()
                .on_truecolor(91, 206, 250)
                .black()
                .bold()
                .style("  INFO "),
            Level::DEBUG => Style::new()
                .on_truecolor(245, 169, 184)
                .black()
                .bold()
                .style(" DEBUG "),
            Level::TRACE => Style::new()
                .on_color(CssColors::White)
                .black()
                .bold()
                .style(" TRACE "),
        };
        write!(writer, "{} ", badge)?;

        let file = event.metadata().file().unwrap_or("?");
        let line = event.metadata().line().unwrap_or(0);
        write!(writer, "{} ", theme::BLUE.style("│"))?;
        write!(
            writer,
            "{}",
            theme::WHITE.dimmed().style(Self::format_path(file, line))
        )?;
        write!(writer, "{}", theme::BLUE.style(" ❯ "))?;

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let mut need_space = false;
        if let Some(msg) = visitor.message {
            write!(writer, "{}", theme::WHITE.style(msg))?;
            need_space = !visitor.fields.is_empty();
        }

        for (k, v) in visitor.fields {
            if need_space {
                write!(writer, " ")?;
            }
            write!(
                writer,
                "{}{}{}",
                theme::PINK.style(k),
                theme::BLUE.style("="),
                theme::WHITE.style(v)
            )?;
            need_space = true;
        }

        if let Some(scope) = ctx.event_scope() {
            write!(writer, " {}", theme::BLUE.style("┇"))?;
            for (i, span) in scope.from_root().enumerate() {
                if i > 0 {
                    write!(writer, "{}", theme::BLUE.style("·"))?;
                }
                Self::write_span(
                    writer.by_ref(),
                    span.name(),
                    span.extensions().get::<FormattedFields<N>>(),
                )?;
            }
        } else if let Some(span) = ctx.lookup_current() {
            write!(writer, " {}", theme::BLUE.style("┇"))?;
            Self::write_span(
                writer.by_ref(),
                span.name(),
                span.extensions().get::<FormattedFields<N>>(),
            )?;
        }

        writeln!(writer)
    }
}

impl AnsiFormatter {
    fn write_span<'a, N>(
        mut writer: Writer<'a>,
        name: &str,
        fields: Option<&FormattedFields<N>>,
    ) -> fmt::Result
    where
        N: for<'b> tracing_subscriber::fmt::FormatFields<'b> + 'static,
    {
        write!(writer, "{}", theme::WHITE.dimmed().style(name))?;

        if let Some(fields) = fields {
            let fields = fields.fields.as_str();
            if !fields.is_empty() {
                write!(writer, "{}", theme::BLUE.dimmed().style("{"))?;
                write!(writer, "{}", theme::WHITE.dimmed().style(fields))?;
                write!(writer, "{}", theme::BLUE.dimmed().style("}"))?;
            }
        }

        Ok(())
    }
}

#[derive(Default)]
#[allow(dead_code)]
struct EventVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl tracing::field::Visit for EventVisitor {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" || field.name() == "msg" {
            self.message = Some(value.to_string());
        } else {
            self.fields
                .push((field.name().to_string(), value.to_string()));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        let val = format!("{:?}", value);
        if field.name() == "message" || field.name() == "msg" {
            let trimmed = if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                &val[1..val.len() - 1]
            } else {
                &val
            };
            self.message = Some(trimmed.to_string());
        } else {
            self.fields.push((field.name().to_string(), val));
        }
    }
}

pub fn rotate_log_file(
    path: &std::path::Path,
    mode: LogRotation,
    _archive_pattern: &str,
) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    match mode {
        LogRotation::None => Ok(()),
        LogRotation::Rename => {
            let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
            let renamed = path.with_extension(format!("{timestamp}.log"));
            std::fs::rename(path, renamed)?;
            Ok(())
        }
        LogRotation::Compress => {
            let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
            let gz_path = path.with_extension(format!("{timestamp}.log.gz"));
            let input = std::fs::read(path)?;
            let output = std::fs::File::create(&gz_path)?;
            let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
            use std::io::Write;
            encoder.write_all(&input)?;
            encoder.finish()?;
            std::fs::remove_file(path)?;
            Ok(())
        }
    }
}
