pub use crate::coordinator::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
use crate::device_list::DeviceList;
pub use crate::FfiCoordinator;
use anyhow::{anyhow, Context};
use flutter_rust_bridge::{frb, RustOpaque, StreamSink, SyncReturn};
pub use frostsnap_coordinator::frostsnap_core;
use frostsnap_coordinator::frostsnap_core::message::SignTask;
pub use frostsnap_coordinator::{DeviceChange, PortDesc};
pub use frostsnap_core::message::{CoordinatorToUserKeyGenMessage, EncodedSignature};
pub use frostsnap_core::{DeviceId, FrostKeyExt, KeyId};
use lazy_static::lazy_static;
use llsdb::LlsDb;
use std::fs::OpenOptions;
pub use std::sync::{Mutex, RwLock};
#[allow(unused)]
use tracing::{event, Level as TLevel};

lazy_static! {
    static ref PORT_EVENT_STREAM: RwLock<Option<StreamSink<PortEvent>>> = RwLock::default();
    static ref DEVICE_LIST: Mutex<(DeviceList, Option<StreamSink<DeviceListUpdate>>)> =
        Default::default();
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
    event!(TLevel::INFO, "logging to stderr");
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

pub fn device_at_index(index: usize) -> SyncReturn<Option<Device>> {
    SyncReturn(DEVICE_LIST.lock().unwrap().0.device_at_index(index))
}

pub fn device_list_state() -> SyncReturn<DeviceListState> {
    SyncReturn(DEVICE_LIST.lock().unwrap().0.state())
}

// pub fn start_signing(devices: Vec<DeviceId>, message: String) ->

pub type SessionHash = [u8; 32];

#[derive(Clone, Debug, Copy)]
#[frb(mirror(EncodedSignature))]
pub struct _EncodedSignature(pub [u8; 64]);

#[derive(Clone, Debug)]
pub struct SigningState {
    pub got_shares: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    // for some reason FRB woudln't allow Option here to empty vec implies not being finished
    pub finished_signatures: Vec<EncodedSignature>,
}

impl SigningState {
    pub fn is_finished(&self) -> SyncReturn<bool> {
        SyncReturn(!self.finished_signatures.is_empty())
    }
}

#[derive(Clone, Debug)]
#[frb(mirror(CoordinatorToUserKeyGenMessage))]
pub enum _CoordinatorToUserKeyGenMessage {
    ReceivedShares { from: DeviceId },
    CheckKeyGen { session_hash: SessionHash },
    KeyGenAck { from: DeviceId },
    FinishedKey { key_id: KeyId },
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

pub fn new_coordinator(db_file: String) -> anyhow::Result<Coordinator> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true) // Creates the file if it does not exist
        .open(db_file.clone())?;

    event!(TLevel::INFO, path = db_file, "initializing database");

    let db =
        LlsDb::load_or_init(file).context(format!("failed to load database from {db_file}"))?;

    Ok(Coordinator(RustOpaque::new(FfiCoordinator::new(db)?)))
}

pub struct Coordinator(pub RustOpaque<FfiCoordinator>);

impl Coordinator {
    pub fn start_thread(&self) -> anyhow::Result<()> {
        self.0.start()
    }

    pub fn announce_available_ports(&self, ports: Vec<PortDesc>) {
        self.0.set_available_ports(ports);
    }

    pub fn switch_to_host_handles_serial(&self) {
        self.0.switch_to_host_handles_serial();
    }

    pub fn update_name_preview(&self, id: DeviceId, name: String) {
        self.0.update_name_preview(id, &name);
    }

    pub fn finish_naming(&self, id: DeviceId, name: String) {
        self.0.finish_naming(id, &name)
    }

    pub fn send_cancel(&self, id: DeviceId) {
        self.0.send_cancel(id);
    }

    pub fn cancel_all(&self) {
        self.0.cancel_all()
    }

    pub fn registered_devices(&self) -> Vec<DeviceId> {
        self.0.registered_devices()
    }

    pub fn key_state(&self) -> SyncReturn<KeyState> {
        SyncReturn(KeyState {
            keys: self.0.frost_keys(),
        })
    }

    pub fn get_key(&self, key_id: KeyId) -> SyncReturn<Option<FrostKey>> {
        SyncReturn(
            self.0
                .frost_keys()
                .into_iter()
                .find(|frost_key| frost_key.id().0 == key_id),
        )
    }

    pub fn start_signing(
        &self,
        key_id: KeyId,
        devices: Vec<DeviceId>,
        message: String,
        stream: StreamSink<SigningState>,
    ) -> anyhow::Result<()> {
        self.0.start_signing(
            key_id,
            devices.into_iter().collect(),
            SignTask::Plain(message.into_bytes()),
            stream,
        )?;
        Ok(())
    }

    pub fn get_signing_state(&self) -> SyncReturn<Option<SigningState>> {
        SyncReturn(self.0.get_signing_state())
    }

    pub fn devices_for_frost_key(&self, frost_key: FrostKey) -> SyncReturn<Vec<Device>> {
        SyncReturn(
            frost_key
                .0
                .devices()
                .map(|id| self.get_device(id).0)
                .collect(),
        )
    }

    pub fn get_device(&self, id: DeviceId) -> SyncReturn<Device> {
        SyncReturn(Device {
            name: self.0.get_device_name(id),
            id,
        })
    }

    pub fn nonces_available(&self, id: DeviceId) -> SyncReturn<usize> {
        SyncReturn(self.0.nonces_left(id).unwrap_or(0))
    }

    pub fn generate_new_key(
        &self,
        threshold: usize,
        devices: Vec<DeviceId>,
        event_stream: StreamSink<CoordinatorToUserKeyGenMessage>,
    ) {
        self.0
            .generate_new_key(devices.into_iter().collect(), threshold, event_stream);
    }

    pub fn can_restore_signing_session(&self, key_id: KeyId) -> SyncReturn<bool> {
        SyncReturn(self.0.can_restore_signing_session(key_id))
    }

    pub fn try_restore_signing_session(
        &self,
        key_id: KeyId,
        stream: StreamSink<SigningState>,
    ) -> anyhow::Result<()> {
        self.0.try_restore_signing_session(key_id, stream)
    }
}

// TODO: remove me?
pub fn echo_key_id(key_id: KeyId) -> KeyId {
    key_id
}
