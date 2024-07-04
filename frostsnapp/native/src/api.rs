use crate::device_list::DeviceList;
pub use crate::ffi_serial_port::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
pub use crate::FfiCoordinator;
pub use crate::FfiQrReader;
use crate::QrDecoderStatus;
use anyhow::{anyhow, Context, Result};
pub use bitcoin::psbt::Psbt as BitcoinPsbt;
pub use bitcoin::Transaction as RTransaction;
use flutter_rust_bridge::{frb, RustOpaque, StreamSink, SyncReturn};
pub use frostsnap_coordinator::bitcoin::wallet::ConfirmationTime;
pub use frostsnap_coordinator::bitcoin::{chain_sync::ChainSync, wallet::FrostsnapWallet};
pub use frostsnap_coordinator::firmware_upgrade::FirmwareUpgradeConfirmState;
pub use frostsnap_coordinator::frostsnap_core;
pub use frostsnap_coordinator::{
    keygen::KeyGenState, signing::SigningState, DeviceChange, PortDesc,
};
use frostsnap_coordinator::{DesktopSerial, UsbSerialManager};
pub use frostsnap_core::message::EncodedSignature;
pub use frostsnap_core::{DeviceId, FrostKeyExt, KeyId, SignTask};
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
use tracing::{event, span, Level};

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
    pub inner: RustOpaque<Arc<RTransaction>>,
    pub confirmation_time: Option<ConfirmationTime>,
}

impl From<frostsnap_coordinator::bitcoin::wallet::Transaction> for Transaction {
    fn from(value: frostsnap_coordinator::bitcoin::wallet::Transaction) -> Self {
        Self {
            net_value: value.net_value,
            inner: RustOpaque::new(value.inner),
            confirmation_time: value.confirmation_time,
        }
    }
}

impl Transaction {
    pub fn txid(&self) -> SyncReturn<String> {
        SyncReturn(self.inner.txid().to_string())
    }
}

#[frb(mirror(ConfirmationTime))]
pub struct _ConfirmationTime {
    pub height: u32,
    pub time: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Device {
    pub name: Option<String>,
    // NOTE: digest should always be present in any device that is actually plugged in
    pub firmware_digest: String,
    pub latest_digest: String,
    pub id: DeviceId,
}

impl Device {
    pub fn ready(&self) -> SyncReturn<bool> {
        SyncReturn(self.name.is_some() && !self.needs_firmware_upgrade().0)
    }

    pub fn needs_firmware_upgrade(&self) -> SyncReturn<bool> {
        SyncReturn(self.firmware_digest != self.latest_digest)
    }
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
pub struct _KeyId(pub [u8; 33]);

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

pub enum LogLevel {
    Debug,
    Info,
}

pub fn turn_stderr_logging_on(level: LogLevel) {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(match level {
            LogLevel::Info => Level::INFO,
            LogLevel::Debug => Level::DEBUG,
        })
        .without_time()
        .pretty()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
    event!(Level::INFO, "logging to stderr");
}

pub fn turn_logcat_logging_on(_level: LogLevel) {
    #[cfg(target_os = "android")]
    {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(match _level {
                LogLevel::Info => tracing::Level::INFO,
                LogLevel::Debug => tracing::Level::DEBUG,
            })
            .without_time()
            .finish();

        let subscriber = {
            use tracing_subscriber::layer::SubscriberExt;
            subscriber.with(tracing_android::layer("rust-frostsnapp").unwrap())
        };
        let _ = tracing::subscriber::set_global_default(subscriber);
        event!(Level::INFO, "frostsnap logging to logcat");
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
    let device = DEVICE_LIST.lock().unwrap().0.get_device(id);
    SyncReturn(device)
}

#[frb(mirror(EncodedSignature))]
pub struct _EncodedSignature(pub [u8; 64]);

#[frb(mirror(SigningState))]
pub struct _SigningState {
    pub got_shares: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    // for some reason FRB woudln't allow Option here to empty vec implies not being finished
    pub finished_signatures: Vec<EncodedSignature>,
}

#[frb(mirror(KeyGenState))]
pub struct _KeyGenState {
    pub devices: Vec<DeviceId>, // not a set for frb compat
    pub got_shares: Vec<DeviceId>,
    pub session_acks: Vec<DeviceId>,
    pub session_hash: Option<[u8; 32]>,
    pub finished: Option<KeyId>,
    pub aborted: Option<String>,
    pub threshold: usize,
}

#[frb(mirror(FirmwareUpgradeConfirmState))]
pub struct _FirmwareUpgradeConfirmState {
    pub confirmations: Vec<DeviceId>,
    pub devices: Vec<DeviceId>,
    pub need_upgrade: Vec<DeviceId>,
    pub abort: bool,
    pub upgrade_ready_to_start: bool,
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
    pub fn get_device(&self, id: DeviceId) -> SyncReturn<Option<Device>> {
        SyncReturn(self.devices.iter().find(|device| device.id == id).cloned())
    }
}

pub struct Wallet {
    pub inner: RustOpaque<Mutex<FrostsnapWallet>>,
    pub wallet_streams: RustOpaque<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>,
    pub chain_sync: RustOpaque<ChainSync>,
}

impl Wallet {
    pub fn sub_tx_state(&self, key_id: KeyId, stream: StreamSink<TxState>) -> Result<()> {
        stream.add(self.tx_state(key_id).0);
        if let Some(existing) = self.wallet_streams.lock().unwrap().insert(key_id, stream) {
            existing.close();
        }

        Ok(())
    }

    pub fn tx_state(&self, key_id: KeyId) -> SyncReturn<TxState> {
        let txs = self.inner.lock().unwrap().list_transactions(key_id);
        SyncReturn(txs.into())
    }

    pub fn sync_txids(
        &self,
        key_id: KeyId,
        txids: Vec<String>,
        stream: StreamSink<f64>,
    ) -> Result<()> {
        let span = span!(Level::DEBUG, "syncing txids");
        event!(Level::INFO, "starting sync");
        let _enter = span.enter();
        let chain_sync = self.chain_sync.clone();
        let start = Instant::now();

        let sync_request = {
            let wallet = self.inner.lock().unwrap();
            let txids = txids
                .into_iter()
                .map(|txid| bitcoin::Txid::from_str(&txid).unwrap())
                .collect();
            let sync_request = wallet.sync_txs(txids);
            let total = sync_request.txids.len();
            let mut i = 0;
            let inspect_stream = stream.clone();
            sync_request.inspect_txids(move |_txid| {
                inspect_stream.add(i as f64 / total as f64);
                i += 1;
            })
        };

        let update = chain_sync.sync(sync_request)?;
        let mut wallet = self.inner.lock().unwrap();
        let something_changed = wallet.finish_sync(update)?;

        if something_changed {
            let txs = wallet.list_transactions(key_id);
            drop(wallet);
            if let Some(wallet_stream) = self.wallet_streams.lock().unwrap().get(&key_id) {
                wallet_stream.add(txs.into());
            }

            event!(
                Level::INFO,
                elapsed = start.elapsed().as_millis(),
                "finished syncing txids with changes"
            );
        } else {
            event!(
                Level::INFO,
                elapsed = start.elapsed().as_millis(),
                "finished syncing txids without chanages"
            );
        }

        stream.add(100.0);
        stream.close();

        Ok(())
    }

    pub fn sync(&self, key_id: KeyId, stream: StreamSink<f64>) -> Result<()> {
        let span = span!(Level::DEBUG, "syncing", key_id = key_id.to_string());
        let _enter = span.enter();
        let start = Instant::now();
        event!(Level::INFO, "starting sync");
        let sync_request = {
            let inspect_stream = stream.clone();
            let wallet = self.inner.lock().unwrap();
            let sync_req = wallet.start_sync(key_id);
            let total = sync_req.spks.len();
            let mut i = 0;
            sync_req.inspect_spks(move |_spk| {
                inspect_stream.add(i as f64 / total as f64);
                i += 1;
            })
        };
        let chain_sync = self.chain_sync.clone();

        let update = chain_sync.sync(sync_request)?;
        let mut wallet = self.inner.lock().unwrap();
        let something_changed = wallet.finish_sync(update)?;

        if something_changed {
            let txs = wallet.list_transactions(key_id);
            drop(wallet);
            if let Some(wallet_stream) = self.wallet_streams.lock().unwrap().get(&key_id) {
                wallet_stream.add(txs.into());
            }

            event!(Level::INFO, "finished with changes");
        } else {
            event!(
                Level::INFO,
                elapsed = start.elapsed().as_millis(),
                "finished without changes"
            );
        }

        stream.add(100.0);
        stream.close();

        Ok(())
    }

    pub fn next_address(&self, key_id: KeyId) -> Result<Address> {
        self.inner
            .lock()
            .unwrap()
            .next_address(key_id)
            .map(Into::into)
    }

    pub fn addresses_state(&self, key_id: KeyId) -> SyncReturn<Vec<Address>> {
        SyncReturn(
            self.inner
                .lock()
                .unwrap()
                .list_addresses(key_id)
                .into_iter()
                .map(Into::into)
                .collect(),
        )
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
            template_tx: RustOpaque::new(signing_task),
        };
        Ok(unsigned_tx)
    }

    pub fn broadcast_tx(&self, key_id: KeyId, tx: SignedTx) -> Result<()> {
        match self.chain_sync.broadcast(&*tx.signed_tx) {
            Ok(_) => {
                event!(
                    Level::INFO,
                    tx = tx.signed_tx.txid().to_string(),
                    "transaction successfully broadcast"
                );
                let mut inner = self.inner.lock().unwrap();
                inner.broadcast_success(tx.signed_tx.deref().to_owned());
                let wallet_streams = self.wallet_streams.lock().unwrap();
                if let Some(stream) = wallet_streams.get(&key_id) {
                    let txs = inner.list_transactions(key_id);
                    stream.add(txs.into());
                }
                Ok(())
            }
            Err(e) => {
                use bitcoin::consensus::Encodable;
                use frostsnap_core::schnorr_fun::fun::hex;
                let mut buf = vec![];
                tx.signed_tx.consensus_encode(&mut buf).unwrap();
                let hex_tx = hex::encode(&buf);
                event!(
                    Level::ERROR,
                    tx = tx.signed_tx.txid().to_string(),
                    hex = hex_tx,
                    error = e.to_string(),
                    "unable to broadcast"
                );
                Err(e)
            }
        }
    }

    pub fn psbt_to_unsigned_tx(&self, psbt: Psbt, key_id: KeyId) -> Result<SyncReturn<UnsignedTx>> {
        let template = self
            .inner
            .lock()
            .unwrap()
            .psbt_to_tx_template(&*psbt.inner, key_id.to_root_pubkey().expect("valid key id"))?;

        Ok(SyncReturn(UnsignedTx {
            template_tx: RustOpaque::new(template),
        }))
    }
}

pub fn load(db_file: String) -> anyhow::Result<(Coordinator, Wallet, BitcoinContext)> {
    let usb_manager = UsbSerialManager::new(Box::new(DesktopSerial), crate::FIRMWARE);
    _load(db_file, usb_manager)
}

pub fn load_host_handles_serial(
    db_file: String,
) -> anyhow::Result<(Coordinator, FfiSerial, Wallet, BitcoinContext)> {
    let ffi_serial = FfiSerial::default();
    let usb_manager = UsbSerialManager::new(Box::new(ffi_serial.clone()), crate::FIRMWARE);
    let (coord, wallet, bitcoin_context) = _load(db_file, usb_manager)?;
    Ok((coord, ffi_serial, wallet, bitcoin_context))
}

fn _load(
    db_file: String,
    usb_serial_manager: UsbSerialManager,
) -> Result<(Coordinator, Wallet, BitcoinContext)> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true) // Creates the file if it does not exist
        .truncate(false)
        .open(db_file.clone())?;

    event!(Level::INFO, path = db_file, "initializing database");

    let db =
        LlsDb::load_or_init(file).context(format!("failed to load database from {db_file}"))?;

    let db = Arc::new(Mutex::new(db));

    let coordinator = FfiCoordinator::new(db.clone(), usb_serial_manager)?;
    let wallet = FrostsnapWallet::load_or_init(db.clone(), bitcoin::Network::Signet)
        .with_context(|| format!("loading wallet from data in {db_file}"))?;
    let coordinator = Coordinator(RustOpaque::new(coordinator));
    let chain_sync = ChainSync::new(wallet.network)?;
    let bitcoin_context = BitcoinContext {
        network: RustOpaque::new(wallet.network),
    };

    let wallet = Wallet {
        inner: RustOpaque::new(Mutex::new(wallet)),
        chain_sync: RustOpaque::new(chain_sync),
        wallet_streams: RustOpaque::new(Default::default()),
    };

    Ok((coordinator, wallet, bitcoin_context))
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

    pub fn display_backup(
        &self,
        id: DeviceId,
        key_id: KeyId,
        stream: StreamSink<()>,
    ) -> Result<()> {
        self.0.request_display_backup(id, key_id, stream)?;
        Ok(())
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
            SignTask::Plain { message },
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
            SignTask::BitcoinTransaction(unsigned_tx.template_tx.deref().clone()),
            stream,
        )?;
        Ok(())
    }

    pub fn nonces_available(&self, id: DeviceId) -> SyncReturn<usize> {
        SyncReturn(self.0.nonces_left(id).unwrap_or(0))
    }

    pub fn generate_new_key(
        &self,
        threshold: usize,
        devices: Vec<DeviceId>,
        event_stream: StreamSink<KeyGenState>,
    ) -> anyhow::Result<()> {
        self.0
            .generate_new_key(devices.into_iter().collect(), threshold, event_stream)
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

    pub fn start_firmware_upgrade(
        &self,
        sink: StreamSink<FirmwareUpgradeConfirmState>,
    ) -> Result<()> {
        self.0.begin_upgrade_firmware(sink)?;
        Ok(())
    }

    pub fn upgrade_firmware_digest(&self) -> SyncReturn<String> {
        SyncReturn(self.0.upgrade_firmware_digest().to_string())
    }

    pub fn cancel_protocol(&self) {
        self.0.cancel_protocol()
    }

    pub fn enter_firmware_upgrade_mode(&self, progress: StreamSink<f32>) -> Result<()> {
        self.0.enter_firmware_upgrade_mode(progress)
    }
}

/// The point of this is to keep bitcoin API functionalities that don't require the wallet separate
/// from it.
pub struct BitcoinContext {
    pub network: RustOpaque<bitcoin::Network>,
}

impl BitcoinContext {
    pub fn descriptor_for_key(&self, key_id: KeyId) -> SyncReturn<String> {
        let descriptor = frostsnap_coordinator::bitcoin::multi_x_descriptor_for_account(
            key_id.to_root_pubkey().expect("valid key id"),
            frostsnap_core::tweak::Account::Segwitv1,
            *self.network,
        );
        SyncReturn(descriptor.to_string())
    }

    pub fn validate_amount(&self, address: String, value: u64) -> SyncReturn<Option<String>> {
        SyncReturn(match bitcoin::Address::from_str(&address) {
            Ok(address) => match address.require_network(*self.network) {
                Ok(address) => {
                    let dust_value = address.script_pubkey().dust_value().to_sat();
                    if value < dust_value {
                        event!(
                            Level::DEBUG,
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

    pub fn validate_destination_address(&self, address: String) -> SyncReturn<Option<String>> {
        SyncReturn(match bitcoin::Address::from_str(&address) {
            Ok(address) => match address.require_network(*self.network) {
                Ok(_) => None,
                Err(e) => Some(e.to_string()),
            },
            Err(e) => Some(e.to_string()),
        })
    }
}

#[derive(Clone)]
pub struct UnsignedTx {
    pub template_tx: RustOpaque<frostsnap_core::bitcoin_transaction::TransactionTemplate>,
}

pub struct SignedTx {
    pub signed_tx: RustOpaque<RTransaction>,
    pub unsigned_tx: UnsignedTx,
}

impl SignedTx {
    pub fn effect(
        &self,
        key_id: KeyId,
        network: RustOpaque<bitcoin::Network>,
    ) -> Result<SyncReturn<EffectOfTx>> {
        self.unsigned_tx.effect(key_id, network)
    }
}

impl UnsignedTx {
    pub fn attach_signatures_to_psbt(&self, signatures: Vec<EncodedSignature>, psbt: Psbt) -> Psbt {
        let mut signed_psbt = psbt.inner.deref().clone();
        let mut signatures = signatures.into_iter();
        for (i, _, _) in self.template_tx.iter_locally_owned_inputs() {
            let signature = signatures.next();
            // we are assuming the signatures are correct here.
            let input = &mut signed_psbt.inputs[i];
            let schnorr_sig = bitcoin::taproot::Signature {
                sig: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.unwrap().0)
                    .unwrap(),
                hash_ty: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            // FIXME: we probably shouldn't be setting the final_script_witness here since that's the job of the PSBT finalizer
            // Instead, it should just set the tap_key_sig and then the finalizer can extract it and set the final_script_witness
            input.final_script_witness = Some(witness);
        }

        Psbt {
            inner: RustOpaque::new(signed_psbt),
        }
    }

    pub fn complete(&self, signatures: Vec<EncodedSignature>) -> SignedTx {
        let mut tx = self.template_tx.to_rust_bitcoin_tx();
        for (txin, signature) in tx.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                sig: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0).unwrap(),
                hash_ty: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.witness = witness;
        }
        SignedTx {
            signed_tx: RustOpaque::new(tx),
            unsigned_tx: self.clone(),
        }
    }

    pub fn effect(
        &self,
        key_id: KeyId,
        network: RustOpaque<bitcoin::Network>,
    ) -> Result<SyncReturn<EffectOfTx>> {
        use frostsnap_core::bitcoin_transaction::RootOwner;
        let fee = self
            .template_tx
            .fee()
            .ok_or(anyhow!("invalid transaction"))?;
        let mut net_value = self.template_tx.net_value();
        let value_for_this_key = net_value
            .remove(&RootOwner::Local(key_id))
            .ok_or(anyhow!("this transaction has no effect on this key"))?;

        let foreign_receiving_addresses = net_value
            .into_iter()
            .filter_map(|(owner, value)| match owner {
                RootOwner::Local(_) => Some(Err(anyhow!(
                    "we don't support spending from multiple different keys"
                ))),
                RootOwner::Foreign(spk) => {
                    if value > 0 {
                        Some(Ok((
                            bitcoin::Address::from_script(spk.as_script(), *network)
                                .expect("will have address form")
                                .to_string(),
                            value as u64,
                        )))
                    } else {
                        None
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(SyncReturn(EffectOfTx {
            net_value: value_for_this_key,
            fee,
            feerate: self.template_tx.feerate(),
            foreign_receiving_addresses,
        }))
    }
}

#[derive(Clone, Debug)]
pub struct Address {
    pub index: u32,
    pub address_string: String,
    pub used: bool,
}

impl From<frostsnap_coordinator::bitcoin::wallet::AddressInfo> for Address {
    fn from(value: frostsnap_coordinator::bitcoin::wallet::AddressInfo) -> Self {
        Self {
            index: value.index,
            address_string: value.address.to_string(),
            used: value.used,
        }
    }
}

pub struct TxState {
    pub txs: Vec<Transaction>,
}

impl From<Vec<frostsnap_coordinator::bitcoin::wallet::Transaction>> for TxState {
    fn from(value: Vec<frostsnap_coordinator::bitcoin::wallet::Transaction>) -> Self {
        Self {
            txs: value.into_iter().map(From::from).collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EffectOfTx {
    pub net_value: i64,
    pub fee: u64,
    pub feerate: Option<f64>,
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

pub struct Psbt {
    pub inner: RustOpaque<BitcoinPsbt>,
}

impl Psbt {
    pub fn to_bytes(&self) -> Result<SyncReturn<Vec<u8>>> {
        let psbt_bytes = self.inner.serialize();
        Ok(SyncReturn(psbt_bytes))
    }
}

pub fn psbt_bytes_to_psbt(psbt_bytes: Vec<u8>) -> Result<SyncReturn<Psbt>> {
    let psbt = match bitcoin::psbt::Psbt::deserialize(&psbt_bytes) {
        Ok(psbt) => psbt,
        Err(e) => {
            event!(
                Level::ERROR,
                "Failed to deserialize PSBT {e} {:?}",
                psbt_bytes
            );
            return Err(anyhow!("Failed to deserialize PSBT: {e}"));
        }
    };
    Ok(SyncReturn(Psbt {
        inner: RustOpaque::new(psbt),
    }))
}

pub struct QrReader(pub RustOpaque<FfiQrReader>);

pub fn new_qr_reader() -> QrReader {
    QrReader(RustOpaque::new(FfiQrReader::new()))
}

impl QrReader {
    pub fn decode_from_bytes(&self, bytes: Vec<u8>) -> Result<QrDecoderStatus> {
        let decoded_qr = crate::camera::read_qr_code_bytes(&bytes)?;
        let decoded_ur = self.0.ingest_ur_strings(decoded_qr)?;
        Ok(decoded_ur)
    }
}
