use flutter_rust_bridge::StreamSink;
use std::io;
use time::{
    format_description::well_known::{iso8601::Config, Iso8601},
    OffsetDateTime,
};
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

pub fn dart_logger<S>(sink: StreamSink<String>) -> impl tracing_subscriber::layer::Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_timer(TimeFormatter)
        .with_writer(move || io::LineWriter::new(DartLogWriter { sink: sink.clone() }))
}

struct TimeFormatter;

const ISO8601_CONFIG: Config = Config::DEFAULT.set_time_precision(
    time::format_description::well_known::iso8601::TimePrecision::Second {
        decimal_digits: None,
    },
);
const TIME_FORMAT: Iso8601<{ ISO8601_CONFIG.encode() }> = Iso8601;

impl tracing_subscriber::fmt::time::FormatTime for TimeFormatter {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = OffsetDateTime::now_utc();
        write!(w, "{}", now.format(&TIME_FORMAT).unwrap())
    }
}
