use crate::device_list::DeviceList;
pub use crate::ffi_serial_port::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
use crate::sink_wrap::SinkWrap;
pub use crate::FfiCoordinator;
pub use crate::{FfiQrEncoder, FfiQrReader, QrDecoderStatus};
use anyhow::{anyhow, Context, Result};
pub use bitcoin::psbt::Psbt as BitcoinPsbt;
pub use bitcoin::Network as RBitcoinNetwork;
pub use bitcoin::Transaction as RTransaction;
use bitcoin::{network, Txid};
use flutter_rust_bridge::{frb, RustOpaque, StreamSink, SyncReturn};
use frostsnap_coordinator::bitcoin::chain_sync::{default_electrum_server, SUPPORTED_NETWORKS};
pub use frostsnap_coordinator::bitcoin::wallet::ConfirmationTime;
pub use frostsnap_coordinator::bitcoin::{
    chain_sync::{ChainClient, ChainStatus, ChainStatusState},
    wallet::FrostsnapWallet,
};
pub use frostsnap_coordinator::firmware_upgrade::FirmwareUpgradeConfirmState;
pub use frostsnap_coordinator::frostsnap_core;
use frostsnap_coordinator::frostsnap_core::coordinator::CoordFrostKey;
use frostsnap_coordinator::frostsnap_core::tweak;
pub use frostsnap_coordinator::verify_address::VerifyAddressProtocolState;
pub use frostsnap_coordinator::{
    check_share::CheckShareState, keygen::KeyGenState, persist::Persisted, signing::SigningState,
    DeviceChange, PortDesc, Settings as RSettings,
};

use frostsnap_coordinator::{DesktopSerial, UsbSerialManager};
pub use frostsnap_core::message::EncodedSignature;
pub use frostsnap_core::{
    AccessStructureId, AccessStructureRef, DeviceId, KeyId, MasterAppkey, SessionHash, SignTask,
};
use lazy_static::lazy_static;
pub use std::collections::BTreeMap;
pub use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
pub use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
pub use std::sync::{Mutex, RwLock};
#[allow(unused)]
use tracing::{event, span, Level};
use tracing_subscriber::layer::SubscriberExt;

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
        SyncReturn(self.inner.compute_txid().to_string())
    }
}

#[frb(mirror(ConfirmationTime))]
pub struct _ConfirmationTime {
    pub height: u32,
    pub time: u64,
}

#[derive(Clone, Debug)]
pub struct ConnectedDevice {
    pub name: Option<String>,
    // NOTE: digest should always be present in any device that is actually plugged in
    pub firmware_digest: String,
    pub latest_digest: Option<String>,
    pub id: DeviceId,
}

impl ConnectedDevice {
    pub fn ready(&self) -> SyncReturn<bool> {
        SyncReturn(self.name.is_some() && !self.needs_firmware_upgrade().0)
    }

    pub fn needs_firmware_upgrade(&self) -> SyncReturn<bool> {
        // We still want to have this return true even when we don't have firmware in the app so we
        // know that the device needs a firmware upgrade (even if we can't give it to them).
        SyncReturn(Some(self.firmware_digest.as_str()) != self.latest_digest.as_deref())
    }
}

#[derive(Clone, Debug)]
pub struct KeyState {
    pub keys: Vec<FrostKey>,
}

#[derive(Clone, Debug)]
pub struct FrostKey(pub(crate) RustOpaque<frostsnap_core::coordinator::CoordFrostKey>);

impl FrostKey {
    pub fn master_appkey(&self) -> SyncReturn<MasterAppkey> {
        SyncReturn(self.0.master_appkey)
    }

    pub fn key_id(&self) -> SyncReturn<KeyId> {
        SyncReturn(self.0.master_appkey.key_id())
    }

    pub fn key_name(&self) -> SyncReturn<String> {
        SyncReturn(self.0.key_name.clone())
    }

    pub fn access_structures(&self) -> SyncReturn<Vec<AccessStructure>> {
        SyncReturn(
            self.0
                .access_structures
                .values()
                .cloned()
                .map(From::from)
                .collect(),
        )
    }
}

impl From<CoordFrostKey> for FrostKey {
    fn from(value: CoordFrostKey) -> Self {
        FrostKey(RustOpaque::new(value))
    }
}

pub struct AccessStructure(
    pub(crate) RustOpaque<frostsnap_core::coordinator::CoordAccessStructure>,
);

impl From<frostsnap_core::coordinator::CoordAccessStructure> for AccessStructure {
    fn from(value: frostsnap_core::coordinator::CoordAccessStructure) -> Self {
        Self(RustOpaque::new(value))
    }
}
impl AccessStructure {
    pub fn threshold(&self) -> SyncReturn<usize> {
        SyncReturn(self.0.threshold())
    }

    pub fn access_structure_ref(&self) -> SyncReturn<AccessStructureRef> {
        SyncReturn(self.0.access_structure_ref())
    }

    pub fn devices(&self) -> SyncReturn<Vec<DeviceId>> {
        SyncReturn(self.0.devices().collect())
    }

    /// Create an identifier that's used to determine compatibility of shamir secret shares.
    /// The first 4
    pub fn id(&self) -> SyncReturn<AccessStructureId> {
        SyncReturn(self.0.access_structure_id())
    }

    pub fn short_id(&self) -> SyncReturn<String> {
        SyncReturn(self.0.access_structure_id().to_string().split_off(8))
    }

    pub fn master_appkey(&self) -> SyncReturn<MasterAppkey> {
        SyncReturn(self.0.master_appkey())
    }
}

#[frb(mirror(PortDesc))]
pub struct _PortDesc {
    pub id: String,
    pub vid: u16,
    pub pid: u16,
}

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

pub fn log(level: LogLevel, message: String) -> SyncReturn<()> {
    // dunno why I can't use runtime log levels here but event! hates it
    match level {
        LogLevel::Debug => event!(Level::DEBUG, "[dart] {}", message),
        LogLevel::Info => event!(Level::INFO, "[dart] {}", message),
    }

    SyncReturn(())
}

pub enum LogLevel {
    Debug,
    Info,
}

impl From<LogLevel> for tracing::Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
        }
    }
}

pub fn turn_stderr_logging_on(level: LogLevel, log_stream: StreamSink<String>) -> Result<()> {
    // Global default subscriber must only be set once.
    if crate::logger::set_dart_logger(log_stream) {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::from(level))
            .without_time()
            .pretty()
            .finish()
            .with(crate::logger::dart_logger());
        let _ = tracing::subscriber::set_global_default(subscriber);
    }
    event!(Level::INFO, "logging to stderr and Dart logger");

    Ok(())
}

#[allow(unused_variables)]
pub fn turn_logcat_logging_on(level: LogLevel, log_stream: StreamSink<String>) -> Result<()> {
    #[cfg(not(target_os = "android"))]
    panic!("Do not call turn_logcat_logging_on outside of android");

    #[cfg(target_os = "android")]
    {
        // Global default subscriber must only be set once.
        if crate::logger::set_dart_logger(log_stream) {
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(match level {
                    LogLevel::Info => tracing::Level::INFO,
                    LogLevel::Debug => tracing::Level::DEBUG,
                })
                .without_time()
                .pretty()
                .finish();

            let subscriber = {
                use tracing_subscriber::layer::SubscriberExt;
                subscriber
                    .with(tracing_android::layer("rust-frostsnapp").unwrap())
                    .with(crate::logger::dart_logger())
            };

            tracing::subscriber::set_global_default(subscriber)?;
        }
        event!(Level::INFO, "frostsnap logging to logcat and Dart logger");
    }

    #[allow(unreachable_code)]
    Ok(())
}

pub fn device_at_index(index: usize) -> SyncReturn<Option<ConnectedDevice>> {
    SyncReturn(DEVICE_LIST.lock().unwrap().0.device_at_index(index))
}

pub fn device_list_state() -> SyncReturn<DeviceListState> {
    SyncReturn(DEVICE_LIST.lock().unwrap().0.state())
}

pub fn get_connected_device(id: DeviceId) -> SyncReturn<Option<ConnectedDevice>> {
    let device = DEVICE_LIST.lock().unwrap().0.get_device(id);
    SyncReturn(device)
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
    pub device: ConnectedDevice,
}

#[derive(Clone, Debug)]
pub struct DeviceListUpdate {
    pub changes: Vec<DeviceListChange>,
    pub state: DeviceListState,
}

#[derive(Clone, Debug)]
pub struct DeviceListState {
    pub devices: Vec<ConnectedDevice>,
    pub state_id: usize,
}

impl DeviceListState {
    pub fn get_device(&self, id: DeviceId) -> SyncReturn<Option<ConnectedDevice>> {
        SyncReturn(self.devices.iter().find(|device| device.id == id).cloned())
    }
}

pub type WalletStreams = Mutex<BTreeMap<MasterAppkey, StreamSink<TxState>>>;

#[derive(Clone)]
pub struct Wallet {
    pub inner: RustOpaque<Arc<Mutex<FrostsnapWallet>>>,
    pub wallet_streams: RustOpaque<Arc<WalletStreams>>,
    pub chain_sync: RustOpaque<ChainClient>,
    pub network: BitcoinNetwork,
}

impl Wallet {
    fn load_or_new(
        app_dir: impl AsRef<Path>,
        network: BitcoinNetwork,
        chain_sync: ChainClient,
    ) -> Result<Wallet> {
        let db_file = network.bdk_file(app_dir);
        let db = rusqlite::Connection::open(&db_file).context(format!(
            "failed to load database from {}",
            db_file.display()
        ))?;

        let db = Arc::new(Mutex::new(db));

        let wallet = FrostsnapWallet::load_or_init(db.clone(), *network.0, chain_sync.clone())
            .with_context(|| format!("loading wallet from data in {}", db_file.display()))?;

        let wallet = Wallet {
            inner: RustOpaque::new(Arc::new(Mutex::new(wallet))),
            chain_sync: RustOpaque::new(chain_sync),
            wallet_streams: RustOpaque::new(Default::default()),
            network,
        };

        Ok(wallet)
    }

    pub fn sub_tx_state(
        &self,
        master_appkey: MasterAppkey,
        stream: StreamSink<TxState>,
    ) -> Result<()> {
        stream.add(self.tx_state(master_appkey).0);
        if let Some(existing) = self
            .wallet_streams
            .lock()
            .unwrap()
            .insert(master_appkey, stream)
        {
            existing.close();
        }

        Ok(())
    }

    pub fn tx_state(&self, master_appkey: MasterAppkey) -> SyncReturn<TxState> {
        let txs = self.inner.lock().unwrap().list_transactions(master_appkey);
        SyncReturn(txs.into())
    }

    pub fn reconnect(&self) {
        self.chain_sync.reconnect();
    }

    pub fn next_address(&self, master_appkey: MasterAppkey) -> Result<Address> {
        self.inner
            .lock()
            .unwrap()
            .next_address(master_appkey)
            .map(Into::into)
    }

    pub fn addresses_state(&self, master_appkey: MasterAppkey) -> SyncReturn<Vec<Address>> {
        SyncReturn(
            self.inner
                .lock()
                .unwrap()
                .list_addresses(master_appkey)
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }

    pub fn search_for_address(
        &self,
        master_appkey: MasterAppkey,
        address_str: String,
        start: u32,
        stop: u32,
    ) -> Option<Address> {
        self.inner
            .lock()
            .unwrap()
            .search_for_address(master_appkey, address_str, start, stop)
            .map(|address_info| Address {
                index: address_info.clone().index,
                address_string: address_info.address.to_string(),
                used: address_info.used,
                external: address_info.external,
            })
    }

    pub fn rebroadcast(&self, txid: String) {
        let txid = Txid::from_str(&txid).expect("Txid must be valid");
        let wallet = self.inner.lock().unwrap();
        if let Some(tx) = wallet.get_tx(txid) {
            if let Err(err) = self.chain_sync.broadcast(tx.as_ref().clone()) {
                tracing::error!("Rebroadcasting {} failed: {}", txid, err);
            };
        }
    }

    pub fn send_to(
        &self,
        master_appkey: MasterAppkey,
        to_address: String,
        value: u64,
        feerate: f64,
    ) -> Result<UnsignedTx> {
        let mut wallet = self.inner.lock().unwrap();
        let to_address = bitcoin::Address::from_str(&to_address)
            .expect("validation should have checked")
            .require_network(wallet.network)
            .expect("validation should have checked");
        let signing_task = wallet.send_to(master_appkey, to_address, value, feerate as f32)?;
        let unsigned_tx = UnsignedTx {
            template_tx: RustOpaque::new(signing_task),
        };
        Ok(unsigned_tx)
    }

    pub fn broadcast_tx(&self, master_appkey: MasterAppkey, tx: SignedTx) -> Result<()> {
        match self.chain_sync.broadcast(tx.signed_tx.deref().clone()) {
            Ok(_) => {
                event!(
                    Level::INFO,
                    tx = tx.signed_tx.compute_txid().to_string(),
                    "transaction successfully broadcast"
                );
                let mut inner = self.inner.lock().unwrap();
                inner.broadcast_success(tx.signed_tx.deref().to_owned());
                let wallet_streams = self.wallet_streams.lock().unwrap();
                if let Some(stream) = wallet_streams.get(&master_appkey) {
                    let txs = inner.list_transactions(master_appkey);
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
                    tx = tx.signed_tx.compute_txid().to_string(),
                    hex = hex_tx,
                    error = e.to_string(),
                    "unable to broadcast"
                );
                Err(e)
            }
        }
    }

    pub fn psbt_to_unsigned_tx(
        &self,
        psbt: Psbt,
        master_appkey: MasterAppkey,
    ) -> Result<SyncReturn<UnsignedTx>> {
        let template = self
            .inner
            .lock()
            .unwrap()
            .psbt_to_tx_template(&psbt.inner, master_appkey)?;

        Ok(SyncReturn(UnsignedTx {
            template_tx: RustOpaque::new(template),
        }))
    }

    pub fn derivation_path_for_address(&self, index: u32, external: bool) -> SyncReturn<String> {
        let account_keychain = if external {
            tweak::BitcoinAccountKeychain::external()
        } else {
            tweak::BitcoinAccountKeychain::internal()
        };
        let bip32_path = tweak::BitcoinBip32Path {
            account_keychain,
            index,
        };

        SyncReturn(
            bip32_path
                .path_segments_from_bitcoin_appkey()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }
}

pub fn load(app_dir: String) -> anyhow::Result<(Coordinator, Settings)> {
    let app_dir = PathBuf::from_str(&app_dir)?;
    let usb_manager = UsbSerialManager::new(Box::new(DesktopSerial), crate::FIRMWARE);
    _load(app_dir, usb_manager)
}

pub fn load_host_handles_serial(
    app_dir: String,
) -> anyhow::Result<(Coordinator, Settings, FfiSerial)> {
    let app_dir = PathBuf::from_str(&app_dir)?;
    let ffi_serial = FfiSerial::default();
    let usb_manager = UsbSerialManager::new(Box::new(ffi_serial.clone()), crate::FIRMWARE);
    let (coord, settings) = _load(app_dir, usb_manager)?;
    Ok((coord, settings, ffi_serial))
}

#[derive(Debug, Clone)]
pub struct BitcoinNetwork(pub RustOpaque<RBitcoinNetwork>);

/// The point of this is to keep bitcoin API functionalities that don't require the wallet separate
/// from it.
impl BitcoinNetwork {
    pub fn signet() -> SyncReturn<BitcoinNetwork> {
        SyncReturn(Self(RustOpaque::new(bitcoin::Network::Signet)))
    }

    pub fn mainnet() -> SyncReturn<BitcoinNetwork> {
        SyncReturn(Self(RustOpaque::new(bitcoin::Network::Bitcoin)))
    }

    pub fn from_string(string: String) -> SyncReturn<Option<BitcoinNetwork>> {
        SyncReturn(
            bitcoin::Network::from_str(&string)
                .ok()
                .map(|network| Self(RustOpaque::new(network))),
        )
    }

    pub fn supported_networks() -> SyncReturn<Vec<BitcoinNetwork>> {
        SyncReturn(
            SUPPORTED_NETWORKS
                .into_iter()
                .map(BitcoinNetwork::from)
                .collect(),
        )
    }

    pub fn name(&self) -> SyncReturn<String> {
        SyncReturn(self.0.to_string())
    }

    pub fn is_mainnet(&self) -> SyncReturn<bool> {
        SyncReturn(bitcoin::NetworkKind::from(*self.0).is_mainnet())
    }

    pub fn descriptor_for_key(&self, master_appkey: MasterAppkey) -> SyncReturn<String> {
        let descriptor = frostsnap_coordinator::bitcoin::multi_x_descriptor_for_account(
            master_appkey,
            frostsnap_core::tweak::BitcoinAccount::default(),
            (*self.0).into(),
        );
        SyncReturn(descriptor.to_string())
    }

    pub fn validate_amount(&self, address: String, value: u64) -> SyncReturn<Option<String>> {
        SyncReturn(match bitcoin::Address::from_str(&address) {
            Ok(address) => match address.require_network(*self.0) {
                Ok(address) => {
                    let dust_value = address.script_pubkey().minimal_non_dust().to_sat();
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
            Ok(address) => match address.require_network(*self.0) {
                Ok(_) => None,
                Err(e) => Some(e.to_string()),
            },
            Err(e) => Some(e.to_string()),
        })
    }

    pub fn default_electrum_server(&self) -> SyncReturn<String> {
        SyncReturn(default_electrum_server(*self.0).to_string())
    }

    fn bdk_file(&self, app_dir: impl AsRef<Path>) -> PathBuf {
        app_dir.as_ref().join(format!("wallet-{}.sql", *self.0))
    }
}

impl From<BitcoinNetwork> for network::Network {
    fn from(value: BitcoinNetwork) -> Self {
        *value.0
    }
}

impl From<network::Network> for BitcoinNetwork {
    fn from(value: network::Network) -> Self {
        BitcoinNetwork(RustOpaque::new(value))
    }
}

fn _load(
    app_dir: PathBuf,
    usb_serial_manager: UsbSerialManager,
) -> Result<(Coordinator, Settings)> {
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
    let coordinator = Coordinator(RustOpaque::new(coordinator));
    let settings = Settings::new(db.clone(), app_dir)?;
    Ok((coordinator, settings))
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

    pub fn display_backup(
        &self,
        id: DeviceId,
        access_structure_ref: AccessStructureRef,
        stream: StreamSink<bool>,
    ) -> Result<()> {
        self.0
            .request_display_backup(id, access_structure_ref, crate::TEMP_KEY, stream)?;
        Ok(())
    }

    pub fn key_state(&self) -> SyncReturn<KeyState> {
        SyncReturn(KeyState {
            keys: self
                .0
                .frost_keys()
                .into_iter()
                .map(FrostKey::from)
                .collect(),
        })
    }

    pub fn sub_key_events(&self, stream: StreamSink<KeyState>) -> Result<()> {
        self.0.sub_key_events(stream);
        Ok(())
    }

    pub fn access_structures_involving_device(
        &self,
        device_id: DeviceId,
    ) -> SyncReturn<Vec<AccessStructureRef>> {
        SyncReturn(
            self.0
                .frost_keys()
                .into_iter()
                .flat_map(|frost_key| {
                    frost_key
                        .access_structures()
                        .filter(|(_, access_structure)| access_structure.contains_device(device_id))
                        .map(|(accsref, _)| accsref)
                        .collect::<Vec<_>>()
                })
                .collect(),
        )
    }

    pub fn start_signing(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: Vec<DeviceId>,
        message: String,
        stream: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            SignTask::Plain { message },
            stream,
            crate::TEMP_KEY,
        )?;
        Ok(())
    }

    pub fn start_signing_tx(
        &self,
        access_structure_ref: AccessStructureRef,
        unsigned_tx: UnsignedTx,
        devices: Vec<DeviceId>,
        stream: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            SignTask::BitcoinTransaction(unsigned_tx.template_tx.deref().clone()),
            stream,
            crate::TEMP_KEY,
        )?;
        Ok(())
    }

    pub fn nonces_available(&self, id: DeviceId) -> SyncReturn<usize> {
        SyncReturn(self.0.nonces_left(id).unwrap_or(0))
    }

    pub fn current_nonce(&self, id: DeviceId) -> SyncReturn<u64> {
        SyncReturn(self.0.current_nonce(id).unwrap_or(0))
    }

    pub fn generate_new_key(
        &self,
        threshold: u16,
        devices: Vec<DeviceId>,
        key_name: String,
        is_mainnet_key: bool,
        event_stream: StreamSink<KeyGenState>,
    ) -> anyhow::Result<()> {
        self.0.generate_new_key(
            devices.into_iter().collect(),
            threshold,
            key_name,
            is_mainnet_key,
            event_stream,
        )
    }

    pub fn persisted_sign_session_description(
        &self,
        key_id: KeyId,
    ) -> SyncReturn<Option<SignTaskDescription>> {
        SyncReturn(self.0.persisted_sign_session_description(key_id))
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

    pub fn upgrade_firmware_digest(&self) -> SyncReturn<Option<String>> {
        SyncReturn(
            self.0
                .upgrade_firmware_digest()
                .map(|digest| digest.to_string()),
        )
    }

    pub fn verify_address(
        &self,
        access_structure_ref: AccessStructureRef,
        address_index: u32,
        master_appkey: MasterAppkey,
        sink: StreamSink<VerifyAddressProtocolState>,
    ) -> Result<()> {
        self.0
            .verify_address(access_structure_ref, address_index, sink, master_appkey)?;
        Ok(())
    }

    pub fn cancel_protocol(&self) {
        self.0.cancel_protocol()
    }

    pub fn enter_firmware_upgrade_mode(&self, progress: StreamSink<f32>) -> Result<()> {
        self.0.enter_firmware_upgrade_mode(progress)
    }

    pub fn get_device_name(&self, id: DeviceId) -> SyncReturn<Option<String>> {
        SyncReturn(self.0.get_device_name(id))
    }

    pub fn final_keygen_ack(&self) -> Result<AccessStructureRef> {
        self.0.final_keygen_ack()
    }

    pub fn check_share_on_device(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        sink: StreamSink<CheckShareState>,
    ) -> Result<()> {
        self.0
            .check_share_on_device(device_id, access_structure_ref, sink, crate::TEMP_KEY)?;
        Ok(())
    }

    pub fn get_access_structure(
        &self,
        as_ref: AccessStructureRef,
    ) -> SyncReturn<Option<AccessStructure>> {
        SyncReturn(
            self.0
                .get_access_structure(as_ref)
                .map(|access_structure| AccessStructure(RustOpaque::new(access_structure))),
        )
    }

    pub fn get_frost_key(&self, key_id: KeyId) -> SyncReturn<Option<FrostKey>> {
        SyncReturn(self.0.get_frost_key(key_id).map(FrostKey::from))
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
        master_appkey: MasterAppkey,
        network: BitcoinNetwork,
    ) -> Result<SyncReturn<EffectOfTx>> {
        self.unsigned_tx.effect(master_appkey, network)
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
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(
                    &signature.unwrap().0,
                )
                .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            };
            input.tap_key_sig = Some(schnorr_sig);
        }

        Psbt {
            inner: RustOpaque::new(signed_psbt),
        }
    }

    pub fn complete(&self, signatures: Vec<EncodedSignature>) -> SignedTx {
        let mut tx = self.template_tx.to_rust_bitcoin_tx();
        for (txin, signature) in tx.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                    .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
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
        master_appkey: MasterAppkey,
        network: BitcoinNetwork,
    ) -> Result<SyncReturn<EffectOfTx>> {
        use frostsnap_core::bitcoin_transaction::RootOwner;
        let fee = self
            .template_tx
            .fee()
            .ok_or(anyhow!("invalid transaction"))?;
        let mut net_value = self.template_tx.net_value();
        let value_for_this_key = net_value
            .remove(&RootOwner::Local(master_appkey))
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
                            bitcoin::Address::from_script(spk.as_script(), *network.0)
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
    pub external: bool,
}

impl From<frostsnap_coordinator::bitcoin::wallet::AddressInfo> for Address {
    fn from(value: frostsnap_coordinator::bitcoin::wallet::AddressInfo) -> Self {
        Self {
            index: value.index,
            address_string: value.address.to_string(),
            used: value.used,
            external: value.external,
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

pub enum SignTaskDescription {
    Plain { message: String },
    // Nostr {
    //     #[bincode(with_serde)]
    //     event: Box<crate::nostr::UnsignedEvent>,
    //     master_appkey: MasterAppkey,
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

pub struct QrEncoder(pub RustOpaque<FfiQrEncoder>);

pub fn new_qr_encoder(bytes: Vec<u8>) -> QrEncoder {
    let mut length_bytes = bytes.len().to_be_bytes().to_vec();
    while length_bytes.len() > 1 && length_bytes[0] == 0 {
        length_bytes.remove(0);
    }

    // prepending OP_PUSHDATA1 and length for CBOR
    let mut encode_bytes = Vec::new();
    encode_bytes.extend_from_slice(&[0x59]);
    encode_bytes.extend_from_slice(&length_bytes);
    encode_bytes.extend_from_slice(&bytes);

    QrEncoder(RustOpaque::new(FfiQrEncoder(Arc::new(Mutex::new(
        ur::Encoder::new(&encode_bytes, 400, "crypto-psbt").unwrap(),
    )))))
}

impl QrEncoder {
    pub fn next(&self) -> SyncReturn<String> {
        SyncReturn(self.0.next().to_uppercase())
    }
}

pub struct Settings {
    pub settings: RustOpaque<Mutex<Persisted<RSettings>>>,
    pub db: RustOpaque<Arc<Mutex<rusqlite::Connection>>>,
    pub chain_clients: RustOpaque<HashMap<RBitcoinNetwork, ChainClient>>,

    pub app_directory: RustOpaque<PathBuf>,
    pub loaded_wallets: RustOpaque<Mutex<HashMap<RBitcoinNetwork, Wallet>>>,

    // streams of settings updates
    pub wallet_settings_stream: RustOpaque<MaybeSink<WalletSettings>>,
    pub developer_settings_stream: RustOpaque<MaybeSink<DeveloperSettings>>,
    pub electrum_settings_stream: RustOpaque<MaybeSink<ElectrumSettings>>,
}

pub type MaybeSink<T> = Mutex<Option<StreamSink<T>>>;

macro_rules! settings_impl {
    ($stream_name:ident, $stream_emit_name:ident, $stream_sub:ident, $type_name:ident) => {
        pub fn $stream_sub(&self, stream: StreamSink<$type_name>) -> Result<()> {
            if let Some(prev) = self.$stream_name.lock().unwrap().replace(stream) {
                prev.close();
            }

            self.$stream_emit_name();
            Ok(())
        }

        fn $stream_emit_name(&self) {
            if let Some(stream) = &*self.$stream_name.lock().unwrap() {
                let settings = self.settings.lock().unwrap();
                stream.add(<$type_name>::from_settings(&*settings));
            }
        }
    };
}

impl Settings {
    fn new(db: Arc<Mutex<rusqlite::Connection>>, app_directory: PathBuf) -> anyhow::Result<Self> {
        let persisted: Persisted<RSettings> = {
            let mut db_ = db.lock().unwrap();
            Persisted::new(&mut *db_, ())?
        };

        let mut loaded_wallets: HashMap<RBitcoinNetwork, Wallet> = Default::default();
        let mut chain_apis = HashMap::new();

        for network in SUPPORTED_NETWORKS {
            let bitcoin_network = BitcoinNetwork::from(network);
            let electrum_url = persisted.get_electrum_server(network);
            let (chain_api, conn_handler) = ChainClient::new();
            let wallet = Wallet::load_or_new(&app_directory, bitcoin_network, chain_api.clone())?;
            conn_handler.run(electrum_url, Arc::clone(&wallet.inner), {
                let wallet_streams = Arc::clone(&wallet.wallet_streams);
                move |master_appkey, txs| {
                    let wallet_streams = wallet_streams.lock().unwrap();
                    if let Some(stream) = wallet_streams.get(&master_appkey) {
                        stream.add(txs.into());
                    }
                }
            });
            loaded_wallets.insert(network, wallet);
            chain_apis.insert(network, chain_api);
        }

        Ok(Self {
            loaded_wallets: RustOpaque::new(Mutex::new(loaded_wallets)),
            settings: RustOpaque::new(Mutex::new(persisted)),
            app_directory: RustOpaque::new(app_directory),
            chain_clients: RustOpaque::new(chain_apis),
            wallet_settings_stream: RustOpaque::new(Default::default()),
            developer_settings_stream: RustOpaque::new(Default::default()),
            electrum_settings_stream: RustOpaque::new(Default::default()),
            db: RustOpaque::new(db),
        })
    }

    settings_impl!(
        developer_settings_stream,
        emit_developer_settings,
        sub_developer_settings,
        DeveloperSettings
    );

    settings_impl!(
        electrum_settings_stream,
        emit_electrum_settings,
        sub_electrum_settings,
        ElectrumSettings
    );

    settings_impl!(
        wallet_settings_stream,
        emit_wallet_settings,
        sub_wallet_settings,
        WalletSettings
    );

    pub fn load_wallet(&self, network: BitcoinNetwork) -> Result<Wallet> {
        let loaded = self.loaded_wallets.lock().unwrap();
        loaded
            .get(&network.0)
            .cloned()
            .ok_or(anyhow!("unsupported network {:?}", network.0))
    }

    pub fn set_wallet_network(&self, key_id: KeyId, network: BitcoinNetwork) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        self.settings
            .lock()
            .unwrap()
            .mutate2(&mut *db, |settings, update| {
                settings.set_wallet_network(key_id, *network.0, update);
                Ok(())
            })?;
        self.emit_wallet_settings();
        Ok(())
    }

    pub fn set_developer_mode(&self, value: bool) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        self.settings
            .lock()
            .unwrap()
            .mutate2(&mut *db, |settings, update| {
                settings.set_developer_mode(value, update);
                Ok(())
            })?;

        self.emit_developer_settings();

        Ok(())
    }

    pub fn check_and_set_electrum_server(
        &self,
        network: BitcoinNetwork,
        url: String,
    ) -> Result<()> {
        let chain_api = self
            .chain_clients
            .get(&*network.0)
            .ok_or_else(|| anyhow!("network not supported {}", *network.0))?;
        chain_api.check_and_set_electrum_server_url(url.clone())?;
        let mut db = self.db.lock().unwrap();
        self.settings
            .lock()
            .unwrap()
            .mutate2(&mut *db, |settings, update| {
                settings.set_electrum_server(*network.0, url, update);
                Ok(())
            })?;

        self.emit_electrum_settings();

        Ok(())
    }

    pub fn subscribe_chain_status(
        &self,
        network: BitcoinNetwork,
        sink: StreamSink<ChainStatus>,
    ) -> Result<()> {
        let chain_api = self
            .chain_clients
            .get(&*network.0)
            .ok_or_else(|| anyhow!("network not supported {}", *network.0))?;

        chain_api.set_status_sink(Box::new(SinkWrap(sink)));
        Ok(())
    }
}

pub struct WalletSettings {
    pub wallet_networks: Vec<(KeyId, BitcoinNetwork)>,
}

impl WalletSettings {
    fn from_settings(settings: &RSettings) -> Self {
        Self {
            wallet_networks: settings
                .wallet_networks
                .clone()
                .into_iter()
                .map(|(master_appkey, network)| {
                    (master_appkey, BitcoinNetwork(RustOpaque::new(network)))
                })
                .collect(),
        }
    }
}

pub struct ElectrumSettings {
    pub electrum_servers: Vec<(BitcoinNetwork, String)>,
}

impl ElectrumSettings {
    fn from_settings(settings: &RSettings) -> Self {
        use bitcoin::Network::*;
        let servers_with_defaults_overridden = [Bitcoin, Signet, Testnet, Regtest]
            .into_iter()
            .map(|network| (network, default_electrum_server(network).to_string()))
            .chain(settings.electrum_servers.clone())
            .collect::<BTreeMap<_, _>>();
        ElectrumSettings {
            electrum_servers: servers_with_defaults_overridden
                .into_iter()
                .map(|(network, url)| (network.into(), url))
                .collect(),
        }
    }
}

pub struct DeveloperSettings {
    pub developer_mode: bool,
}

impl DeveloperSettings {
    fn from_settings(settings: &RSettings) -> Self {
        DeveloperSettings {
            developer_mode: settings.developer_mode,
        }
    }
}

#[frb(mirror(AccessStructureId))]
pub struct _AccessStructureId(pub [u8; 32]);

#[frb(mirror(DeviceId))]
pub struct _DeviceId(pub [u8; 33]);

#[frb(mirror(MasterAppkey))]
pub struct _MasterAppkey(pub [u8; 65]);

#[frb(mirror(KeyId))]
pub struct _KeyId(pub [u8; 32]);

#[frb(mirror(SessionHash))]
pub struct _SessionHash(pub [u8; 32]);

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
    pub threshold: usize,
    pub devices: Vec<DeviceId>, // not a set for frb compat
    pub got_shares: Vec<DeviceId>,
    pub session_acks: Vec<DeviceId>,
    pub all_acks: bool,
    pub session_hash: Option<SessionHash>,
    pub finished: Option<AccessStructureRef>,
    pub aborted: Option<String>,
}

#[frb(mirror(FirmwareUpgradeConfirmState))]
pub struct _FirmwareUpgradeConfirmState {
    pub confirmations: Vec<DeviceId>,
    pub devices: Vec<DeviceId>,
    pub need_upgrade: Vec<DeviceId>,
    pub abort: bool,
    pub upgrade_ready_to_start: bool,
}

#[frb(mirror(AccessStructureRef))]
pub struct _AccessStructureRef {
    pub key_id: KeyId,
    pub access_structure_id: AccessStructureId,
}

#[frb(mirror(CheckShareState))]
pub struct _CheckShareState {
    outcome: Option<bool>,
    abort: Option<String>,
}

#[frb(mirror(ChainStatus))]
pub struct _ChainStatus {
    pub electrum_url: String,
    pub state: ChainStatusState,
}

#[frb(mirror(ChainStatusState))]
pub enum _ChainStatusState {
    Connected,
    Disconnected,
    Connecting,
}

#[frb(mirror(VerifyAddressProtocolState))]
pub struct _VerifyAddressProtocolState {
    pub target_devices: Vec<DeviceId>,
    pub sent_to_devices: Vec<DeviceId>,
}

// XXX: bugs in flutter_rust_bridge mean that sometimes the right code doesn't get emitted unless
// you use it as an argument.
pub fn echo_master_appkey(master_appkey: MasterAppkey) -> MasterAppkey {
    master_appkey
}

pub fn echo_asid(value: AccessStructureId) -> AccessStructureId {
    value
}

pub fn echo_asr(value: AccessStructureRef) -> AccessStructureRef {
    value
}

// In flutter_rust_bridge v2 we can just extend MasterAppkey with this
pub fn master_appkey_ext_to_key_id(master_appkey: MasterAppkey) -> SyncReturn<KeyId> {
    SyncReturn(master_appkey.key_id())
}
