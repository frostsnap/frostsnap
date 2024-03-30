pub use crate::chain_sync::ChainSync;
pub use crate::coordinator::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
use crate::device_list::DeviceList;
pub use crate::FfiCoordinator;
use anyhow::{anyhow, Context, Result};
pub use bdk_chain::bitcoin;
pub use bitcoin::Transaction as RTransaction;
use flutter_rust_bridge::{frb, RustOpaque, StreamSink, SyncReturn};
pub use frostsnap_coordinator::frostsnap_core;
use frostsnap_coordinator::frostsnap_core::message::SignTask;
use frostsnap_coordinator::{DesktopSerial, UsbSerialManager};
pub use frostsnap_coordinator::{DeviceChange, PortDesc};
pub use frostsnap_core::message::{CoordinatorToUserKeyGenMessage, EncodedSignature};
pub use frostsnap_core::{DeviceId, FrostKeyExt, KeyId};
use lazy_static::lazy_static;
use llsdb::LlsDb;
pub use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
pub use std::sync::{Mutex, RwLock};
use std::time::Instant;
#[allow(unused)]
use tracing::{event, span, Level as TLevel};

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
    let (list, stream) = &mut *device_list_and_stream;
    new_stream.add(DeviceListUpdate {
        changes: vec![],
        state: list.state(),
    });
    if let Some(old_stream) = stream.replace(new_stream) {
        old_stream.close();
    }
}

pub(crate) fn emit_event(event: PortEvent) -> Result<()> {
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

pub(crate) fn init_device_names(device_names: HashMap<DeviceId, String>) {
    let mut device_list_and_stream = DEVICE_LIST.lock().unwrap();
    device_list_and_stream.0.init_names(device_names);
}

pub struct Transaction {
    pub net_value: i64,
    pub inner: RustOpaque<RTransaction>,
    pub confirmation_time: Option<ConfirmationTime>,
}

impl Transaction {
    pub fn txid(&self) -> SyncReturn<String> {
        SyncReturn(self.inner.txid().to_string())
    }
}

pub struct ConfirmationTime {
    pub height: u32,
    pub time: u64,
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
pub struct FrostKey(pub(crate) RustOpaque<frostsnap_core::CoordinatorFrostKey>);

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
        SyncReturn(self.0.devices().map(|id| get_device(id).0).collect())
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

pub fn get_device(id: DeviceId) -> SyncReturn<Device> {
    let device = Device {
        name: DEVICE_LIST.lock().unwrap().0.get_device_name(id).cloned(),
        id,
    };
    SyncReturn(device)
}

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
    CheckKeyGen { session_hash: [u8; 32] },
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

    pub fn get_device(&self, id: DeviceId) -> SyncReturn<Option<Device>> {
        SyncReturn(self.devices.iter().find(|device| device.id == id).cloned())
    }
}

pub fn load(db_file: String) -> anyhow::Result<(Coordinator, Wallet)> {
    let usb_manager = UsbSerialManager::new(Box::new(DesktopSerial));
    _load(db_file, usb_manager)
}

pub fn load_host_handles_serial(
    db_file: String,
) -> anyhow::Result<(Coordinator, FfiSerial, Wallet)> {
    let ffi_serial = FfiSerial::default();
    let usb_manager = UsbSerialManager::new(Box::new(ffi_serial.clone()));
    let (coord, wallet) = _load(db_file, usb_manager)?;
    Ok((coord, ffi_serial, wallet))
}

fn _load(db_file: String, usb_serial_manager: UsbSerialManager) -> Result<(Coordinator, Wallet)> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true) // Creates the file if it does not exist
        .truncate(false)
        .open(db_file.clone())?;

    event!(TLevel::INFO, path = db_file, "initializing database");

    let db =
        LlsDb::load_or_init(file).context(format!("failed to load database from {db_file}"))?;

    let db = Arc::new(Mutex::new(db));

    let coordinator = FfiCoordinator::new(db.clone(), usb_serial_manager)?;
    let persist_core_handle = coordinator.persist_core_handle();
    let wallet = crate::wallet::_Wallet::load_or_init(
        db.clone(),
        bitcoin::Network::Signet,
        persist_core_handle,
    )?;
    let coordinator = Coordinator(RustOpaque::new(coordinator));
    let chain_sync = ChainSync::new(wallet.network)?;
    let wallet = Wallet::new(wallet, chain_sync);

    Ok((coordinator, wallet))
}

#[derive(Debug, Clone)]
pub struct FfiSerial {
    pub(crate) available_ports: RustOpaque<Arc<Mutex<Vec<PortDesc>>>>,
}

impl Default for FfiSerial {
    fn default() -> Self {
        Self {
            available_ports: RustOpaque::new(Default::default()),
        }
    }
}

impl FfiSerial {
    pub fn set_available_ports(&self, ports: Vec<PortDesc>) {
        *self.available_ports.lock().unwrap() = ports
    }
}

pub struct Coordinator(pub RustOpaque<FfiCoordinator>);

impl Coordinator {
    pub fn start_thread(&self) -> Result<()> {
        self.0.start()
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

    pub fn display_backup(&self, id: DeviceId, key_id: KeyId) {
        self.0.request_display_backup(id, key_id)
    }

    pub fn key_state(&self) -> SyncReturn<KeyState> {
        SyncReturn(KeyState {
            keys: self.0.frost_keys(),
        })
    }

    pub fn sub_key_events(&self, stream: StreamSink<KeyState>) -> Result<()> {
        self.0.sub_key_events(stream);
        Ok(())
    }

    pub fn get_key(&self, key_id: KeyId) -> SyncReturn<Option<FrostKey>> {
        SyncReturn(
            self.0
                .frost_keys()
                .into_iter()
                .find(|frost_key| frost_key.id().0 == key_id),
        )
    }

    pub fn keys_for_device(&self, device_id: DeviceId) -> SyncReturn<Vec<KeyId>> {
        SyncReturn(
            self.0
                .frost_keys()
                .into_iter()
                .filter_map(|frost_key| {
                    if frost_key
                        .devices()
                        .0
                        .into_iter()
                        .any(|device| device.id == device_id)
                    {
                        Some(frost_key.id().0)
                    } else {
                        None
                    }
                })
                .collect(),
        )
    }

    pub fn start_signing(
        &self,
        key_id: KeyId,
        devices: Vec<DeviceId>,
        message: String,
        stream: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            key_id,
            devices.into_iter().collect(),
            SignTask::Plain {
                message: message.into_bytes(),
            },
            stream,
        )?;
        Ok(())
    }

    pub fn start_signing_tx(
        &self,
        key_id: KeyId,
        unsigned_tx: UnsignedTx,
        devices: Vec<DeviceId>,
        stream: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            key_id,
            devices.into_iter().collect(),
            SignTask::Transaction(unsigned_tx.task.deref().clone()),
            stream,
        )?;
        Ok(())
    }

    pub fn get_signing_state(&self) -> SyncReturn<Option<SigningState>> {
        SyncReturn(self.0.get_signing_state())
    }

    pub fn nonces_available(&self, id: DeviceId) -> SyncReturn<usize> {
        SyncReturn(self.0.nonces_left(id).unwrap_or(0))
    }

    pub fn generate_new_key(
        &self,
        threshold: usize,
        devices: Vec<DeviceId>,
        event_stream: StreamSink<CoordinatorToUserKeyGenMessage>,
    ) -> anyhow::Result<()> {
        self.0
            .generate_new_key(devices.into_iter().collect(), threshold, event_stream)
    }

    pub fn can_restore_signing_session(&self, key_id: KeyId) -> SyncReturn<bool> {
        SyncReturn(self.0.can_restore_signing_session(key_id))
    }

    pub fn persisted_sign_session_description(
        &self,
        key_id: KeyId,
    ) -> Result<SyncReturn<Option<SignTaskDescription>>> {
        self.0
            .persisted_sign_session_description(key_id)
            .map(SyncReturn)
    }

    pub fn try_restore_signing_session(
        &self,
        key_id: KeyId,
        stream: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.try_restore_signing_session(key_id, stream)
    }
}

pub struct Wallet {
    pub inner: RustOpaque<Mutex<crate::wallet::_Wallet>>,
    pub wallet_streams: RustOpaque<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>,
    pub chain_sync: RustOpaque<ChainSync>,
}

impl Wallet {
    fn new(wallet: crate::wallet::_Wallet, chain_sync: ChainSync) -> Self {
        Self {
            inner: RustOpaque::new(Mutex::new(wallet)),
            wallet_streams: RustOpaque::new(Default::default()),
            chain_sync: RustOpaque::new(chain_sync),
        }
    }

    pub fn sub_tx_state(&self, key_id: KeyId, stream: StreamSink<TxState>) -> Result<()> {
        stream.add(self.tx_state(key_id).0);
        if let Some(existing) = self.wallet_streams.lock().unwrap().insert(key_id, stream) {
            existing.close();
        }

        Ok(())
    }

    pub fn tx_state(&self, key_id: KeyId) -> SyncReturn<TxState> {
        let txs = self.inner.lock().unwrap().list_transactions(key_id);
        SyncReturn(TxState { txs })
    }

    pub fn sync_txids(
        &self,
        key_id: KeyId,
        txids: Vec<String>,
        stream: StreamSink<f64>,
    ) -> Result<()> {
        let span = span!(TLevel::DEBUG, "syncing txids");
        event!(TLevel::INFO, "starting sync");
        let _enter = span.enter();
        let chain_sync = self.chain_sync.clone();
        let start = Instant::now();

        let mut sync_request = {
            let wallet = self.inner.lock().unwrap();
            let txids = txids
                .into_iter()
                .map(|txid| bitcoin::Txid::from_str(&txid).unwrap())
                .collect();
            wallet.sync_txs(txids)
        };

        let inspect_stream = stream.clone();

        sync_request.inspect_all(move |_item, _i, total_processed, total| {
            inspect_stream.add(total_processed as f64 / total as f64);
        });

        let update = chain_sync.sync(sync_request)?;
        let mut wallet = self.inner.lock().unwrap();
        let something_changed = wallet.finish_sync(update)?;

        if something_changed {
            let txs = wallet.list_transactions(key_id);
            drop(wallet);
            if let Some(wallet_stream) = self.wallet_streams.lock().unwrap().get(&key_id) {
                wallet_stream.add(TxState { txs });
            }

            event!(
                TLevel::INFO,
                elapsed = start.elapsed().as_millis(),
                "finished syncing txids with changes"
            );
        } else {
            event!(
                TLevel::INFO,
                elapsed = start.elapsed().as_millis(),
                "finished syncing txids without chanages"
            );
        }

        stream.add(100.0);
        stream.close();

        Ok(())
    }

    pub fn sync(&self, key_id: KeyId, stream: StreamSink<f64>) -> Result<()> {
        let span = span!(TLevel::DEBUG, "syncing", key_id = key_id.to_string());
        let _enter = span.enter();
        let start = Instant::now();
        event!(TLevel::INFO, "starting sync");
        let mut sync_request = {
            let wallet = self.inner.lock().unwrap();
            wallet.start_sync(key_id)
        };
        let chain_sync = self.chain_sync.clone();
        let inspect_stream = stream.clone();

        sync_request.inspect_all(move |_item, _i, total_processed, total| {
            inspect_stream.add(total_processed as f64 / total as f64);
        });

        let update = chain_sync.sync(sync_request)?;
        let mut wallet = self.inner.lock().unwrap();
        let something_changed = wallet.finish_sync(update)?;

        if something_changed {
            let txs = wallet.list_transactions(key_id);
            drop(wallet);
            if let Some(wallet_stream) = self.wallet_streams.lock().unwrap().get(&key_id) {
                wallet_stream.add(TxState { txs });
            }

            event!(TLevel::INFO, "finished with changes");
        } else {
            event!(
                TLevel::INFO,
                elapsed = start.elapsed().as_millis(),
                "finished without changes"
            );
        }

        stream.add(100.0);
        stream.close();

        Ok(())
    }

    pub fn next_address(&self, key_id: KeyId) -> Result<Address> {
        self.inner.lock().unwrap().next_address(key_id)
    }

    pub fn addresses_state(&self, key_id: KeyId) -> SyncReturn<Vec<Address>> {
        SyncReturn(self.inner.lock().unwrap().list_addresses(key_id))
    }

    pub fn validate_destination_address(&self, address: String) -> SyncReturn<Option<String>> {
        SyncReturn(match bitcoin::Address::from_str(&address) {
            Ok(address) => match address.require_network(self.inner.lock().unwrap().network) {
                Ok(_) => None,
                Err(e) => Some(e.to_string()),
            },
            Err(e) => Some(e.to_string()),
        })
    }

    pub fn validate_amount(&self, address: String, value: u64) -> SyncReturn<Option<String>> {
        SyncReturn(match bitcoin::Address::from_str(&address) {
            Ok(address) => match address.require_network(self.inner.lock().unwrap().network) {
                Ok(address) => {
                    let dust_value = address.script_pubkey().dust_value().to_sat();
                    if value < dust_value {
                        event!(
                            TLevel::DEBUG,
                            value = value,
                            dust_value = dust_value,
                            "address validation rejected"
                        );
                        Some(format!("Too small to send. Must be at least {dust_value}"))
                    } else {
                        None
                    }
                }
                Err(_e) => None,
            },
            Err(_e) => None,
        })
    }

    pub fn send_to(
        &self,
        key_id: KeyId,
        to_address: String,
        value: u64,
        feerate: f64,
    ) -> Result<UnsignedTx> {
        let mut wallet = self.inner.lock().unwrap();
        let to_address = bitcoin::Address::from_str(&to_address)
            .expect("validation should have checked")
            .require_network(wallet.network)
            .expect("validation should have checked");
        let signing_task = wallet.send_to(key_id, to_address, value, feerate as f32)?;
        let unsigned_tx = UnsignedTx {
            task: RustOpaque::new(signing_task),
        };
        Ok(unsigned_tx)
    }

    pub fn complete_unsigned_tx(
        &self,
        unsigned_tx: UnsignedTx,
        signatures: Vec<EncodedSignature>,
    ) -> Result<SyncReturn<SignedTx>> {
        let tx = self
            .inner
            .lock()
            .unwrap()
            .complete_tx_sign_task(unsigned_tx.task.deref().clone(), signatures)?;
        Ok(SyncReturn(SignedTx {
            inner: RustOpaque::new(tx),
        }))
    }

    pub fn broadcast_tx(&self, key_id: KeyId, tx: SignedTx) -> Result<()> {
        match self.chain_sync.broadcast(&tx.inner) {
            Ok(_) => {
                event!(
                    TLevel::INFO,
                    tx = tx.inner.txid().to_string(),
                    "transaction successfully broadcast"
                );
                let mut inner = self.inner.lock().unwrap();
                inner.broadcast_success(tx.inner.deref().to_owned());
                let wallet_streams = self.wallet_streams.lock().unwrap();
                if let Some(stream) = wallet_streams.get(&key_id) {
                    let txs = inner.list_transactions(key_id);
                    stream.add(TxState { txs });
                }
                Ok(())
            }
            Err(e) => {
                use bdk_chain::bitcoin::consensus::Encodable;
                use frostsnap_core::schnorr_fun::fun::hex;
                let mut buf = vec![];
                tx.inner.consensus_encode(&mut buf).unwrap();
                let hex_tx = hex::encode(&buf);
                event!(
                    TLevel::ERROR,
                    tx = tx.inner.txid().to_string(),
                    hex = hex_tx,
                    error = e.to_string(),
                    "unable to broadcast"
                );
                Err(e)
            }
        }
    }

    pub fn effect_of_tx(
        &self,
        key_id: KeyId,
        tx: RustOpaque<RTransaction>,
    ) -> Result<SyncReturn<EffectOfTx>> {
        let inner = self.inner.lock().unwrap();
        let fee = inner.fee(&tx)?;
        Ok(SyncReturn(EffectOfTx {
            net_value: inner.net_value(key_id, &tx),
            fee,
            feerate: fee as f64 / (tx.weight().to_wu() as f64 / 4.0),
            foreign_receiving_addresses: inner
                .spends_outside(&tx)
                .into_iter()
                .map(|(spk, value)| {
                    (
                        bitcoin::Address::from_script(&spk, inner.network)
                            .map(|address| address.to_string())
                            .unwrap_or(spk.to_hex_string()),
                        value,
                    )
                })
                .collect(),
        }))
    }
}

pub struct SignedTx {
    pub inner: RustOpaque<RTransaction>,
}

impl SignedTx {
    pub fn tx(&self) -> SyncReturn<RustOpaque<RTransaction>> {
        SyncReturn(self.inner.clone())
    }
}

pub struct UnsignedTx {
    pub task: RustOpaque<frostsnap_core::message::TransactionSignTask>,
}

impl UnsignedTx {
    pub fn tx(&self) -> SyncReturn<RustOpaque<RTransaction>> {
        SyncReturn(RustOpaque::new(self.task.tx_template.clone()))
    }
}

#[derive(Clone, Debug)]
pub struct Address {
    pub index: u32,
    pub address_string: String,
    pub used: bool,
}

pub struct TxState {
    pub txs: Vec<Transaction>,
}

#[derive(Clone, Debug)]
pub struct EffectOfTx {
    pub net_value: i64,
    pub fee: u64,
    pub feerate: f64,
    pub foreign_receiving_addresses: Vec<(String, u64)>,
}

// TODO: remove me?
pub fn echo_key_id(key_id: KeyId) -> KeyId {
    key_id
}

pub enum SignTaskDescription {
    Plain { message: String },
    // Nostr {
    //     #[bincode(with_serde)]
    //     event: Box<crate::nostr::UnsignedEvent>,
    //     key_id: KeyId,
    // }, // 1 nonce & sig
    Transaction { unsigned_tx: UnsignedTx },
}
