use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flutter_rust_bridge::StreamSink;
use tracing::{Event, Level, Subscriber};
// use tracing_subscriber::field::VisitFmt;
use tracing_subscriber::fmt::format::{PrettyVisitor, Writer};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::api::LogEntry;

pub struct StreamLogsToDart {
    pub sink: StreamSink<crate::api::LogEntry>,
}

impl<S: Subscriber> Layer<S> for StreamLogsToDart
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        let mut text = String::new();
        {
            let writer = Writer::new(&mut text);
            let mut visitor = PrettyVisitor::new(writer, true);
            // write!(visitor.writer(), "{}: ", module_path).unwrap();
            event.record(&mut visitor);

            /* File and line trace */

            // write!(visitor.writer(), "\n    at {}:{}", file, line).unwrap();
        }

        /* Extended scope trace */

        // Stream the formatted log message to Dart.
        self.log(*metadata.level(), &text);
    }
}

impl StreamLogsToDart {
    pub fn log(&self, level: Level, content: &str) {
        let entry = {
            let time_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_millis() as i64;

            let level = level.to_string();

            LogEntry {
                time_millis,
                level,
                content: content.to_owned(),
            }
        };

        self.sink.add(entry);
    }
}
