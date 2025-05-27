use super::{
    backup_manager::BackupManager, coordinator::Coordinator, log::LogLevel, port::FfiSerial,
    settings::Settings,
};
use crate::{
    coordinator::FfiCoordinator,
    frb_generated::{RustAutoOpaque, StreamSink},
};
use anyhow::{Context as _, Result};
use frostsnap_coordinator::{DesktopSerial, UsbSerialManager};
use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};
use tracing_subscriber::layer::SubscriberExt as _;

impl super::Api {
    pub fn turn_logging_on(&self, level: LogLevel, log_stream: StreamSink<String>) -> Result<()> {
        // Global default subscriber must only be set once.
        if crate::logger::set_dart_logger(log_stream) {
            #[cfg(not(target_os = "android"))]
            let subscriber = {
                event!(Level::INFO, "logging to stderr and Dart logger");
                tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::from(level))
                    .without_time()
                    .pretty()
                    .finish()
                    .with(crate::logger::dart_logger())
            };

            #[cfg(target_os = "android")]
            let subscriber = {
                event!(Level::INFO, "logging to logcat and dart logger");
                use tracing_logcat::{LogcatMakeWriter, LogcatTag};
                use tracing_subscriber::fmt::format::Format; // For configuring event formatting

                let writer = LogcatMakeWriter::new(LogcatTag::Fixed("frostsnap/rust".to_owned())) // <--- Corrected!
                    .expect("Failed to initialize logcat writer");

                tracing_subscriber::fmt()
                    .event_format(
                        Format::default()
                            .with_level(true) // Keep level in message (e.g., "[INFO]")
                            .with_target(true) // Keep target in message (e.g., "my_module::function")
                            .without_time() // Logcat adds its own time, so avoid duplication
                            .compact(), // Or .pretty() for multi-line details in complex events
                    )
                    .with_writer(writer) // This sends the formatted output to Android's Logcat via LogcatMakeWriter
                    .with_ansi(false) // Logcat doesn't process ANSI escape codes for colors
                    .with_max_level(tracing::Level::from(level)) // Apply the desired max level to this formatter
                    .finish() // This completes the FmtSubscriber
                    .with(crate::logger::dart_logger()) // Chain your Dart logger as another layer
            };

            tracing::subscriber::set_global_default(subscriber)
                .expect("Failed to set global tracing subscriber");

            tracing::info!("Rust tracing initialized (Android)!");
        }
        Ok(())
    }

    pub fn load_host_handles_serial(
        &self,
        app_dir: String,
    ) -> Result<(Coordinator, AppCtx, FfiSerial)> {
        let app_dir = PathBuf::from_str(&app_dir)?;
        let ffi_serial = FfiSerial::default();
        let usb_manager = UsbSerialManager::new(Box::new(ffi_serial.clone()), crate::FIRMWARE);
        let (coord, app_state) = _load(app_dir, usb_manager)?;
        Ok((coord, app_state, ffi_serial))
    }

    pub fn load(&self, app_dir: String) -> anyhow::Result<(Coordinator, AppCtx)> {
        let app_dir = PathBuf::from_str(&app_dir)?;
        let usb_manager = UsbSerialManager::new(Box::new(DesktopSerial), crate::FIRMWARE);
        _load(app_dir, usb_manager)
    }
}

fn _load(app_dir: PathBuf, usb_serial_manager: UsbSerialManager) -> Result<(Coordinator, AppCtx)> {
    let db_file = app_dir.join("frostsnap.sqlite");
    event!(
        Level::INFO,
        path = db_file.display().to_string(),
        "initializing database"
    );
    let db = rusqlite::Connection::open(&db_file).with_context(|| {
        event!(
            Level::ERROR,
            path = db_file.display().to_string(),
            "failed to load database"
        );
        format!("failed to load database from {}", db_file.display())
    })?;
    let db = Arc::new(Mutex::new(db));

    let coordinator = FfiCoordinator::new(db.clone(), usb_serial_manager)?;
    let coordinator = Coordinator(coordinator);
    let app_state = AppCtx {
        settings: RustAutoOpaque::new(Settings::new(db.clone(), app_dir)?),
        backup_manager: RustAutoOpaque::new(BackupManager::new(db.clone())?),
    };
    println!("loaded db");

    Ok((coordinator, app_state))
}

pub struct AppCtx {
    pub settings: RustAutoOpaque<Settings>,
    pub backup_manager: RustAutoOpaque<BackupManager>,
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    flutter_rust_bridge::setup_default_user_utils();
}
