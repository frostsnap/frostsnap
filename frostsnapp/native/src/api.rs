pub use crate::coordinator::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
pub use crate::FfiCoordinator;
use anyhow::anyhow;
use flutter_rust_bridge::{frb, RustOpaque, StreamSink, SyncReturn};
pub use frostsnap_coordinator::frostsnap_core;
pub use frostsnap_coordinator::{DeviceChange, PortDesc};
pub use frostsnap_core::message::CoordinatorToUserKeyGenMessage;
pub use frostsnap_core::schnorr_fun;
pub use frostsnap_core::{CoordinatorFrostKeyState, DeviceId, FrostKeyExt, KeyId};
use lazy_static::lazy_static;
pub use schnorr_fun::fun::marker::Normal;
pub use std::sync::{Mutex, RwLock};
#[allow(unused)]
use tracing::{event, Level as TLevel};

lazy_static! {
    static ref PORT_EVENT_STREAM: RwLock<Option<StreamSink<PortEvent>>> = RwLock::default();
    static ref DEVICE_EVENT_STREAM: RwLock<Option<StreamSink<Vec<DeviceChange>>>> =
        RwLock::default();
    static ref PENDING_DEVICE_EVENTS: Mutex<Vec<DeviceChange>> = Default::default();
    static ref KEYGEN_STREAM: Mutex<Option<StreamSink<CoordinatorToUserKeyGenMessage>>> =
        Default::default();
    static ref COORDINATOR: FfiCoordinator = FfiCoordinator::new();
    static ref KEY_EVENT_STREAM: Mutex<Option<StreamSink<KeyState>>> = Default::default();
}

pub fn sub_port_events(event_stream: StreamSink<PortEvent>) {
    let mut v = PORT_EVENT_STREAM
        .write()
        .expect("lock must not be poisoned");
    *v = Some(event_stream);
}

pub fn sub_device_events(stream: StreamSink<Vec<DeviceChange>>) {
    {
        let mut device_event_stream = DEVICE_EVENT_STREAM.write().unwrap();
        if let Some(existing) = device_event_stream.replace(stream) {
            existing.close();
        }
    }
    emit_device_events(vec![]);
}

pub fn sub_key_events(stream: StreamSink<KeyState>) {
    let mut key_event_stream = KEY_EVENT_STREAM.lock().unwrap();
    if let Some(existing) = key_event_stream.replace(stream) {
        existing.close();
    }
}

pub(crate) fn emit_event(event: PortEvent) -> anyhow::Result<()> {
    let stream = PORT_EVENT_STREAM.read().expect("lock must not be poisoned");

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

#[derive(Clone, Debug)]
pub struct KeyState {
    pub keys: Vec<FrostKey>,
}

#[derive(Clone, Debug)]
pub struct FrostKey(pub RustOpaque<frostsnap_core::schnorr_fun::frost::FrostKey<Normal>>);

impl FrostKey {
    pub fn threshold(&self) -> SyncReturn<usize> {
        SyncReturn(self.0.threshold())
    }

    pub fn id(&self) -> SyncReturn<KeyId> {
        SyncReturn(self.0.key_id())
    }

    pub fn name(&self) -> SyncReturn<String> {
        SyncReturn("KEY NAMES NOT IMPLEMENTED".into())
    }
}

#[frb(mirror(PortDesc))]
pub struct _PortDesc {
    pub id: String,
    pub vid: u16,
    pub pid: u16,
}

#[frb(mirror(DeviceId))]
pub struct _DeviceId(pub [u8; 33]);

#[frb(mirror(KeyId))]
pub struct _KeyId(pub [u8; 32]);

#[derive(Debug)]
pub enum PortEvent {
    Open { request: PortOpen },
    Write { request: PortWrite },
    Read { request: PortRead },
    BytesToRead { request: PortBytesToRead },
}

#[frb(mirror(DeviceChange))]
#[derive(Debug, Clone)]
pub enum _DeviceChange {
    Added {
        id: DeviceId,
    },
    Renamed {
        id: DeviceId,
        old_name: String,
        new_name: String,
    },
    NeedsName {
        id: DeviceId,
    },
    Registered {
        id: DeviceId,
        name: String,
    },
    Disconnected {
        id: DeviceId,
    },
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

pub fn announce_available_ports(ports: Vec<PortDesc>) {
    COORDINATOR.set_available_ports(ports);
}

pub fn switch_to_host_handles_serial() {
    COORDINATOR.switch_to_host_handles_serial();
}

pub fn update_name_preview(id: DeviceId, name: String) {
    COORDINATOR.update_name_preview(id, &name);
}

pub fn finish_naming(id: DeviceId, name: String) {
    COORDINATOR.finish_naming(id, &name);
}

pub fn send_cancel(id: DeviceId) {
    COORDINATOR.send_cancel(id);
}

pub fn cancel_all() {
    COORDINATOR.cancel_all()
}

pub fn registered_devices() -> Vec<DeviceId> {
    COORDINATOR.registered_devices()
}

pub fn start_coordinator_thread() {
    COORDINATOR.start()
}

pub fn key_state() -> SyncReturn<KeyState> {
    SyncReturn(KeyState {
        keys: COORDINATOR.frost_keys(),
    })
}

pub type SessionHash = [u8; 32];

#[derive(Clone, Debug)]
#[frb(mirror(CoordinatorToUserKeyGenMessage))]
pub enum _CoordinatorToUserKeyGenMessage {
    ReceivedShares { id: DeviceId },
    CheckKeyGen { session_hash: SessionHash },
    KeyGenAck { id: DeviceId },
    FinishedKey { key_id: KeyId },
}

pub(crate) fn emit_keygen_event(event: CoordinatorToUserKeyGenMessage) {
    let stream = KEYGEN_STREAM.lock().expect("lock must not be poisoned");
    let stream = stream.as_ref().expect("generate new key must be called");

    let is_finished = matches!(event, CoordinatorToUserKeyGenMessage::FinishedKey { .. });

    if !stream.add(event) {
        event!(TLevel::ERROR, "failed to emit keygen event");
    }

    if is_finished {
        let mut key_events = KEY_EVENT_STREAM.lock().unwrap();
        if let Some(key_events) = &mut *key_events {
            key_events.add(KeyState {
                keys: COORDINATOR.frost_keys(),
            });
        }
        stream.close();
    }
}

pub fn generate_new_key(
    threshold: usize,
    devices: Vec<DeviceId>,
    event_stream: StreamSink<CoordinatorToUserKeyGenMessage>,
) {
    let mut global_keygen_stream = KEYGEN_STREAM.lock().unwrap();
    *global_keygen_stream = Some(event_stream);
    COORDINATOR.generate_new_key(devices.into_iter().collect(), threshold);
}
