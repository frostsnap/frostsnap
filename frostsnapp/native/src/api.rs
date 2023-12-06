pub use crate::coordinator::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
use crate::device_list::DeviceList;
pub use crate::FfiCoordinator;
use anyhow::anyhow;
use flutter_rust_bridge::{frb, RustOpaque, StreamSink, SyncReturn};
pub use frostsnap_coordinator::frostsnap_core;
use frostsnap_coordinator::frostsnap_core::message::SignTask;
pub use frostsnap_coordinator::{DeviceChange, PortDesc};
pub use frostsnap_core::message::{
    CoordinatorToUserKeyGenMessage, CoordinatorToUserSigningMessage, EncodedSignature,
};
pub use frostsnap_core::schnorr_fun;
pub use frostsnap_core::{CoordinatorFrostKeyState, DeviceId, FrostKeyExt, KeyId};
use lazy_static::lazy_static;
pub use schnorr_fun::{fun::marker::Normal, Signature};
pub use std::sync::{Mutex, RwLock};
#[allow(unused)]
use tracing::{event, Level as TLevel};

lazy_static! {
    static ref PORT_EVENT_STREAM: RwLock<Option<StreamSink<PortEvent>>> = RwLock::default();
    static ref DEVICE_LIST: Mutex<(DeviceList, Option<StreamSink<DeviceListUpdate>>)> =
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

pub fn sub_device_events(new_stream: StreamSink<DeviceListUpdate>) {
    let mut device_list_and_stream = DEVICE_LIST.lock().unwrap();
    let (_, stream) = &mut *device_list_and_stream;

    if let Some(old_stream) = stream.replace(new_stream) {
        old_stream.close();
    }
}

pub fn sub_key_events(stream: StreamSink<KeyState>) {
    let mut key_event_stream = KEY_EVENT_STREAM.lock().unwrap();
    if let Some(existing) = key_event_stream.replace(stream) {
        existing.close();
    }
}

pub fn emit_key_event(event: KeyState) {
    let mut key_events = KEY_EVENT_STREAM.lock().unwrap();

    if let Some(key_events) = &mut *key_events {
        key_events.add(event);
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

pub(crate) fn emit_device_events(new_events: Vec<DeviceChange>) {
    let mut device_list_and_stream = DEVICE_LIST.lock().unwrap();
    let (list, stream) = &mut *device_list_and_stream;
    let list_events = list.consume_manager_event(new_events);
    if let Some(stream) = stream {
        if !list_events.is_empty() {
            stream.add(DeviceListUpdate {
                state: list.state(),
                changes: list_events,
            });
        }
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub name: Option<String>,
    pub id: DeviceId,
}

#[derive(Clone, Debug)]
pub struct KeyState {
    pub keys: Vec<FrostKey>,
}

#[derive(Clone, Debug)]
pub struct FrostKey(pub(crate) RustOpaque<frostsnap_core::CoordinatorFrostKeyState>);

impl FrostKey {
    pub fn threshold(&self) -> SyncReturn<usize> {
        SyncReturn(self.0.frost_key().threshold())
    }

    pub fn id(&self) -> SyncReturn<KeyId> {
        SyncReturn(self.0.frost_key().key_id())
    }

    pub fn name(&self) -> SyncReturn<String> {
        SyncReturn("KEY NAMES NOT IMPLEMENTED".into())
    }

    pub fn devices(&self) -> SyncReturn<Vec<Device>> {
        let device_names = COORDINATOR.device_names();
        SyncReturn(
            self.0
                .devices()
                .map(|id| Device {
                    name: device_names.get(&id).cloned(),
                    id,
                })
                .collect(),
        )
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

pub fn get_key(key_id: KeyId) -> SyncReturn<Option<FrostKey>> {
    SyncReturn(
        COORDINATOR
            .frost_keys()
            .into_iter()
            .find(|frost_key| frost_key.id().0 == key_id),
    )
}

pub fn device_at_index(index: usize) -> SyncReturn<Option<Device>> {
    SyncReturn(DEVICE_LIST.lock().unwrap().0.device_at_index(index))
}

pub fn device_list_state() -> SyncReturn<DeviceListState> {
    SyncReturn(DEVICE_LIST.lock().unwrap().0.state())
}

pub fn start_signing(
    key_id: KeyId,
    devices: Vec<DeviceId>,
    message: String,
    stream: StreamSink<CoordinatorToUserSigningMessage>,
) -> anyhow::Result<()> {
    COORDINATOR.start_signing(
        key_id,
        devices.into_iter().collect(),
        SignTask::Plain(message.into_bytes()),
        stream,
    )
}

// pub fn start_signing(devices: Vec<DeviceId>, message: String) ->

pub type SessionHash = [u8; 32];

#[derive(Clone, Debug, Copy)]
#[frb(mirror(EncodedSignature))]
pub struct _EncodedSignature(pub [u8; 64]);

#[derive(Clone, Debug)]
#[frb(mirror(CoordinatorToUserSigningMessage))]
pub enum _CoordinatorToUserSigningMessage {
    GotShare { from: DeviceId },
    Signed { signatures: Vec<EncodedSignature> },
}

#[derive(Clone, Debug)]
#[frb(mirror(CoordinatorToUserKeyGenMessage))]
pub enum _CoordinatorToUserKeyGenMessage {
    ReceivedShares { from: DeviceId },
    CheckKeyGen { session_hash: SessionHash },
    KeyGenAck { from: DeviceId },
    FinishedKey { key_id: KeyId },
}

pub fn generate_new_key(
    threshold: usize,
    devices: Vec<DeviceId>,
    event_stream: StreamSink<CoordinatorToUserKeyGenMessage>,
) {
    COORDINATOR.generate_new_key(devices.into_iter().collect(), threshold, event_stream);
}

#[derive(Clone, Debug)]
pub enum DeviceListChangeKind {
    Added,
    Removed,
    Named,
}

#[derive(Clone, Debug)]
pub struct DeviceListChange {
    pub kind: DeviceListChangeKind,
    pub index: usize,
    pub device: Device,
}

#[derive(Clone, Debug)]
pub struct DeviceListUpdate {
    pub changes: Vec<DeviceListChange>,
    pub state: DeviceListState,
}

#[derive(Clone, Debug)]
pub struct DeviceListState {
    pub devices: Vec<Device>,
    pub state_id: usize,
}

impl DeviceListState {
    pub fn named_devices(&self) -> SyncReturn<Vec<DeviceId>> {
        SyncReturn(
            self.devices
                .iter()
                .filter_map(|device| {
                    let _name = device.name.as_ref()?;
                    Some(device.id)
                })
                .collect(),
        )
    }
}
