use flutter_rust_bridge::frb;
use tracing::{event, Level};

#[frb(sync)]
pub fn log(level: LogLevel, message: String) {
    match level {
        LogLevel::Debug => event!(Level::DEBUG, "[dart] {}", message),
        LogLevel::Info => event!(Level::INFO, "[dart] {}", message),
        LogLevel::Error => event!(Level::ERROR, "[dart] {}", message),
    }
}

pub enum LogLevel {
    Debug,
    Info,
    Error,
}

impl From<LogLevel> for tracing::Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}
