use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::{Event, Level, Subscriber};
// use tracing_subscriber::field::VisitFmt;
use tracing_subscriber::fmt::format::{PrettyVisitor, Writer};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::api::{LogEntry, LOG_EVENT_STREAM};

pub struct StreamLogsToDart;

impl<S: Subscriber> Layer<S> for StreamLogsToDart
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        // let module_path = metadata.module_path().unwrap_or_default();
        // let file = metadata.file().unwrap_or_default();
        // let line = metadata.line().unwrap_or_default();

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

        // if let Some(scope) = ctx.event_scope(event) {
        //     for span in scope {
        //         let mut span_text = String::new();
        //         {
        //             let mut span_writer = Writer::new(&mut span_text);
        //             write!(span_writer, "\n    in {}:{}", file, span.name()).unwrap();

        //             if let Some(fields) = span.extensions().get::<FormattedFields<DefaultFields>>()
        //             {
        //                 write!(
        //                     span_writer,
        //                     " with {}: {}",
        //                     span.metadata().name(),
        //                     fields.fields.as_str()
        //                 )
        //                 .unwrap();
        //             }
        //         }
        //         text.push_str(&span_text);
        //     }
        // }

        // Stream the formatted log message to Dart.
        Self::log(*metadata.level(), &text);
    }
}

impl StreamLogsToDart {
    pub fn log(level: Level, content: &str) {
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

        if let Some(sink) = &*LOG_EVENT_STREAM.read().unwrap() {
            sink.add(entry);
        }
    }
}
