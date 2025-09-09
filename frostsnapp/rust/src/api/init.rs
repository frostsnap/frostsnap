use super::{
    backup_manager::BackupManager, coordinator::Coordinator, log::LogLevel,
    psbt_manager::PsbtManager, settings::Settings,
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
use tracing_subscriber::filter::Targets;
use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt, Registry};

impl super::Api {
    pub fn turn_logging_on(&self, level: LogLevel, log_stream: StreamSink<String>) -> Result<()> {
        // Global default subscriber must only be set once.
        if crate::logger::set_dart_logger(log_stream) {
            let targets = Targets::new()
                .with_target("nusb", Level::ERROR /* nusb makes spurious warnings */)
                .with_target("bdk_electrum_streaming", Level::WARN)
                .with_default(Level::from(level));

            #[cfg(not(target_os = "android"))]
            {
                let fmt_layer = tracing_subscriber::fmt::layer().without_time().pretty();

                Registry::default()
                    .with(targets.clone())
                    .with(fmt_layer)
                    .with(crate::logger::dart_logger())
                    .try_init()?;
            }

            #[cfg(target_os = "android")]
            {
                use tracing_logcat::{LogcatMakeWriter, LogcatTag};
                use tracing_subscriber::{fmt::format::Format, layer::Layer};

                let writer = LogcatMakeWriter::new(LogcatTag::Fixed("frostsnap/rust".to_owned()))
                    .expect("logcat writer");

                let fmt_layer = tracing_subscriber::fmt::layer()
                    .event_format(Format::default().with_target(true).without_time().compact())
                    .with_writer(writer)
                    .with_ansi(false)
                    .with_filter(targets.clone());

                Registry::default()
                    .with(targets.clone())
                    .with(fmt_layer)
                    .with(crate::logger::dart_logger())
                    .try_init()?;
            }

            event!(Level::INFO, "Rust tracing initialised");
        }
        Ok(())
    }

    // Android-specific function that returns FfiSerial
    pub fn load_host_handles_serial(
        &self,
        app_dir: String,
    ) -> Result<(Coordinator, AppCtx, super::port::FfiSerial)> {
        use super::port::FfiSerial;
        let app_dir = PathBuf::from_str(&app_dir)?;
        let ffi_serial = FfiSerial::default();
        let usb_manager = UsbSerialManager::new(Box::new(ffi_serial.clone()), crate::FIRMWARE);
        let (coord, app_state) = load_internal(app_dir, usb_manager)?;
        Ok((coord, app_state, ffi_serial))
    }

    // Desktop function using DesktopSerial
    pub fn load(&self, app_dir: String) -> anyhow::Result<(Coordinator, AppCtx)> {
        let app_dir = PathBuf::from_str(&app_dir)?;
        let usb_manager = UsbSerialManager::new(Box::new(DesktopSerial), crate::FIRMWARE);
        load_internal(app_dir, usb_manager)
    }
}

fn load_internal(
    app_dir: PathBuf,
    usb_serial_manager: UsbSerialManager,
) -> Result<(Coordinator, AppCtx)> {
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
        psbt_manager: RustAutoOpaque::new(PsbtManager::new(db.clone())),
    };
    println!("loaded db");

    Ok((coordinator, app_state))
}

pub struct AppCtx {
    pub settings: RustAutoOpaque<Settings>,
    pub backup_manager: RustAutoOpaque<BackupManager>,
    pub psbt_manager: RustAutoOpaque<PsbtManager>,
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Default utilities - feel free to customize
    flutter_rust_bridge::setup_default_user_utils();
}
