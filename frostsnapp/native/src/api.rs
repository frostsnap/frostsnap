pub use crate::coordinator::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
pub use crate::FfiCoordinator;
use anyhow::anyhow;
use flutter_rust_bridge::{frb, RustOpaque, StreamSink};
pub use frostsnap_coordinator::PortDesc;
use lazy_static::lazy_static;
pub use std::os::fd::RawFd;
pub use std::sync::{Mutex, RwLock};
#[allow(unused)]
use tracing::{event, Level as TLevel};

lazy_static! {
    static ref EVENT_STREAM: RwLock<Option<StreamSink<PortEvent>>> = RwLock::default();
    static ref DEVICE_EVENT_STREAM: RwLock<Option<StreamSink<Vec<DeviceChange>>>> =
        RwLock::default();
    static ref PENDING_DEVICE_EVENTS: Mutex<Vec<DeviceChange>> = Default::default();
}

pub fn sub_port_events(event_stream: StreamSink<PortEvent>) {
    let mut v = EVENT_STREAM.write().expect("lock must not be poisoned");
    *v = Some(event_stream);
}

pub fn sub_device_events(stream: StreamSink<Vec<DeviceChange>>) {
    *DEVICE_EVENT_STREAM.write().unwrap() = Some(stream);
    emit_device_events(vec![]);
}

pub(crate) fn emit_event(event: PortEvent) -> anyhow::Result<()> {
    let stream = EVENT_STREAM.read().expect("lock must not be poisoned");

    let stream = stream.as_ref().expect("init_events must be called first");

    if !stream.add(event) {
        return Err(anyhow!("failed to emit event"));
    }

    Ok(())
}

pub(crate) fn emit_device_events(mut new_events: Vec<DeviceChange>) {
    let mut events = PENDING_DEVICE_EVENTS.lock().unwrap();
    events.append(&mut new_events);

    if let Some(stream) = DEVICE_EVENT_STREAM.read().unwrap().as_ref() {
        let events = std::mem::take(&mut *events);
        stream.add(events);
    }
}

#[frb(mirror(PortDesc))]
pub struct _PortDesc {
    pub id: String,
    pub vid: u16,
    pub pid: u16,
}

pub type DeviceId = String;

#[derive(Debug)]
pub enum PortEvent {
    Open { request: PortOpen },
    Write { request: PortWrite },
    Read { request: PortRead },
    BytesToRead { request: PortBytesToRead },
}

#[derive(Debug)]
pub enum DeviceChange {
    Added { id: DeviceId },
    Registered { id: DeviceId, label: String },
    Disconnected { id: DeviceId },
}

#[derive(Debug)]
pub struct PortOpen {
    pub id: String,
    pub baud_rate: u32,
    pub ready: RustOpaque<PortOpenSender>,
}

impl PortOpen {
    pub fn satisfy(&self, err: Option<String>) {
        let result = match err {
            Some(err) => Err(frostsnap_coordinator::PortOpenError::Other(err.into())),
            None => Ok(()),
        };

        let _ = self.ready.0.send(result);
    }
}

#[derive(Debug)]
pub struct PortRead {
    pub id: String,
    pub len: usize,
    pub ready: RustOpaque<PortReadSender>,
}

impl PortRead {
    pub fn satisfy(&self, bytes: Vec<u8>, err: Option<String>) {
        let result = match err {
            Some(err) => Err(err),
            None => Ok(bytes),
        };

        let _ = self.ready.0.send(result);
    }
}

#[derive(Debug)]
pub struct PortWrite {
    pub id: String,
    pub bytes: Vec<u8>,
    pub ready: RustOpaque<PortWriteSender>,
}

impl PortWrite {
    pub fn satisfy(&self, err: Option<String>) {
        let result = match err {
            Some(err) => Err(err),
            None => Ok(()),
        };

        let _ = self.ready.0.send(result);
    }
}

#[derive(Debug)]
pub struct PortBytesToRead {
    pub id: String,
    pub ready: RustOpaque<PortBytesToReadSender>,
}

impl PortBytesToRead {
    pub fn satisfy(&self, bytes_to_read: u32) {
        let _ = self.ready.0.send(bytes_to_read);
    }
}

pub fn new_ffi_coordinator(host_handles_serial: bool) -> RustOpaque<FfiCoordinator> {
    RustOpaque::new(FfiCoordinator::new(host_handles_serial))
}

pub enum Level {
    Debug,
    Info,
}

pub fn turn_stderr_logging_on(level: Level) {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(match level {
            Level::Info => TLevel::INFO,
            Level::Debug => TLevel::DEBUG,
        })
        .without_time()
        .pretty()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub fn turn_logcat_logging_on(_level: Level) {
    #[cfg(target_os = "android")]
    {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(match _level {
                Level::Info => tracing::Level::INFO,
                Level::Debug => tracing::Level::DEBUG,
            })
            .without_time()
            .finish();

        let subscriber = {
            use tracing_subscriber::layer::SubscriberExt;
            subscriber.with(tracing_android::layer("rust-frostsnapp").unwrap())
        };
        let _ = tracing::subscriber::set_global_default(subscriber);
        event!(TLevel::INFO, "frostsnap logging to logcat");
    }
    #[cfg(not(target_os = "android"))]
    panic!("Do not call turn_logcat_logging_on outside of android");
}

pub fn announce_available_ports(coordinator: RustOpaque<FfiCoordinator>, ports: Vec<PortDesc>) {
    coordinator.set_available_ports(ports);
}

pub fn set_device_label(
    coordinator: RustOpaque<FfiCoordinator>,
    device_id: DeviceId,
    label: String,
) {
    coordinator.set_device_label(device_id, label)
}
