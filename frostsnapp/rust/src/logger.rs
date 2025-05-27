use std::{io, sync::RwLock};

use crate::frb_generated::StreamSink;
use lazy_static::lazy_static;
use time::{
    format_description::well_known::{iso8601::Config, Iso8601},
    OffsetDateTime,
};
use tracing_subscriber::registry::LookupSpan;

lazy_static! {
    static ref LOG_SINK: RwLock<Option<StreamSink<String>>> = Default::default();
}

#[derive(Clone)]
struct DartLogWriter;

impl io::Write for DartLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let string = String::from_utf8_lossy(buf);
        let newline_stripped = string.trim_end_matches('\n');
        let sink_lock = LOG_SINK.read().unwrap();
        let _ = sink_lock
            .as_ref()
            .unwrap()
            .add(newline_stripped.to_string());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Set the global [`StreamSink`] used by [`dart_logger`].
///
/// Returns whether this is the first call.
pub fn set_dart_logger(sink: StreamSink<String>) -> bool {
    let mut sink_lock = LOG_SINK.write().unwrap();
    let is_new = sink_lock.is_none();
    *sink_lock = Some(sink);
    is_new
}

/// Obtain the Dart logger.
///
/// [`set_dart_logger`] must be called atleast once before calling this method.
pub fn dart_logger<S>() -> impl tracing_subscriber::layer::Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_file(false)
        .with_line_number(false)
        .with_target(false)
        .with_timer(TimeFormatter)
        .with_writer(move || io::LineWriter::new(DartLogWriter))
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
