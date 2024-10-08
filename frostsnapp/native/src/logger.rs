use flutter_rust_bridge::StreamSink;
use std::io;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};
use tracing_subscriber::registry::LookupSpan;

#[derive(Clone)]
pub struct DartLogWriter {
    sink: StreamSink<String>,
}

impl io::Write for DartLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let string = String::from_utf8_lossy(buf);
        let newline_stripped = string.trim_end_matches('\n');
        self.sink.add(newline_stripped.to_string());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn dart_logger<S>(
    sink: StreamSink<String>,
    utc_offset: i32,
) -> impl tracing_subscriber::layer::Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_timer(TimeFormatter { utc_offset })
        .with_writer(move || io::LineWriter::new(DartLogWriter { sink: sink.clone() }))
}

struct TimeFormatter {
    utc_offset: i32,
}

impl tracing_subscriber::fmt::time::FormatTime for TimeFormatter {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let offset = UtcOffset::from_whole_seconds(self.utc_offset).unwrap_or(UtcOffset::UTC);
        let utc_now = OffsetDateTime::now_utc();
        let now = utc_now.to_offset(offset);
        write!(
            w,
            "{}",
            now.format(&Rfc3339)
                .unwrap()
                .split('.')
                .next()
                .unwrap_or("")
        )
    }
}
