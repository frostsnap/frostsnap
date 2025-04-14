pub use crate::ffi_serial_port::{
    PortBytesToReadSender, PortOpenSender, PortReadSender, PortWriteSender,
};
use crate::sink_wrap::SinkWrap;
pub use crate::FfiCoordinator;
pub use crate::{FfiQrEncoder, FfiQrReader, QrDecoderStatus};
use anyhow::{anyhow, Context, Result};
pub use bitcoin::address::NetworkChecked as RNetworkChecked;
use bitcoin::hex::DisplayHex as _;
pub use bitcoin::psbt::Psbt as BitcoinPsbt;
pub use bitcoin::Address as RAddress;
pub use bitcoin::Network as RBitcoinNetwork;
pub use bitcoin::OutPoint as ROutPoint;
pub use bitcoin::ScriptBuf as RScriptBuf;
pub use bitcoin::Transaction as RTransaction;
pub use bitcoin::TxOut as RTxOut;
use bitcoin::{network, Txid};
use flutter_rust_bridge::{frb, RustOpaque, StreamSink, SyncReturn};
use frostsnap_coordinator::bitcoin::chain_sync::{default_electrum_server, SUPPORTED_NETWORKS};
pub use frostsnap_coordinator::bitcoin::wallet::ConfirmationTime;
pub use frostsnap_coordinator::bitcoin::{
    chain_sync::{ChainClient, ChainStatus, ChainStatusState},
    wallet::CoordSuperWallet,
};
pub use frostsnap_coordinator::firmware_upgrade::FirmwareUpgradeConfirmState;
pub use frostsnap_coordinator::frostsnap_core;
use frostsnap_coordinator::frostsnap_core::coordinator::CoordFrostKey;
use frostsnap_coordinator::frostsnap_core::device::KeyPurpose;
pub use frostsnap_coordinator::verify_address::VerifyAddressProtocolState;
pub use frostsnap_coordinator::{
    check_share::CheckShareState, keygen::KeyGenState, persist::Persisted, signing::SigningState,
    DeviceChange, PortDesc, Settings as RSettings,
};

pub use frostsnap_coordinator::backup_run::{BackupRun, BackupState};
use frostsnap_coordinator::{BackupMutation, DesktopSerial, UsbSerialManager};
pub use frostsnap_core::message::EncodedSignature;
pub use frostsnap_core::{
    coordinator::ActiveSignSession as RActiveSignSession, AccessStructureId, AccessStructureRef,
    DeviceId, KeyId, KeygenId, MasterAppkey, SessionHash, SignSessionId, WireSignTask,
};
use lazy_static::lazy_static;
pub use std::collections::BTreeMap;
pub use std::collections::HashMap;
pub use std::collections::HashSet;
use std::ops::Deref;
use std::path::Path;
pub use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
pub use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
#[allow(unused)]
use tracing::{event, span, Level};
use tracing_subscriber::layer::SubscriberExt;

lazy_static! {
    static ref PORT_EVENT_STREAM: RwLock<Option<StreamSink<PortEvent>>> = RwLock::default();
}

pub fn sub_port_events(event_stream: StreamSink<PortEvent>) {
    let mut v = PORT_EVENT_STREAM
        .write()
        .expect("lock must not be poisoned");
    *v = Some(event_stream);
}

pub(crate) fn emit_event(event: PortEvent) -> Result<()> {
    let stream = PORT_EVENT_STREAM.read().expect("lock must not be poisoned");

    let stream = stream.as_ref().expect("init_events must be called first");

    if !stream.add(event) {
        return Err(anyhow!("failed to emit event"));
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct TxOutInfo {
    pub vout: u32,
    pub amount: u64,
    pub script_pubkey: RustOpaque<RScriptBuf>,
    pub is_mine: bool,
}

impl TxOutInfo {
    pub fn address(&self, network: BitcoinNetwork) -> SyncReturn<Option<String>> {
        fn _address(info: &TxOutInfo, network: BitcoinNetwork) -> Option<String> {
            let address = bitcoin::Address::from_script(&info.script_pubkey, *network.0).ok()?;
            Some(address.to_string())
        }
        SyncReturn(_address(self, network))
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub inner: RustOpaque<RTransaction>,
    pub txid: String,
    pub confirmation_time: Option<ConfirmationTime>,
    pub last_seen: Option<u64>,
    pub prevouts: RustOpaque<HashMap<ROutPoint, RTxOut>>,
    pub is_mine: RustOpaque<HashSet<RScriptBuf>>,
}

impl From<frostsnap_coordinator::bitcoin::wallet::Transaction> for Transaction {
    fn from(value: frostsnap_coordinator::bitcoin::wallet::Transaction) -> Self {
        Self {
            inner: RustOpaque::from(value.inner),
            txid: value.txid.to_string(),
            confirmation_time: value.confirmation_time,
            last_seen: value.last_seen,
            prevouts: RustOpaque::new(value.prevouts),
            is_mine: RustOpaque::new(value.is_mine),
        }
    }
}

impl Transaction {
    /// Computes the sum of all inputs, or only those whose previous output script pubkey is in
    /// `filter`, if provided. The result is `None` if any input is missing a previous output.
    fn _sum_inputs(&self, filter: Option<&HashSet<RScriptBuf>>) -> Option<u64> {
        let prevouts = self
            .inner
            .input
            .iter()
            .map(|txin| self.prevouts.get(&txin.previous_output))
            .collect::<Option<Vec<_>>>()?;
        Some(
            prevouts
                .into_iter()
                .filter(|prevout| {
                    match &filter {
                        Some(filter) => filter.contains(prevout.script_pubkey.as_script()),
                        // No filter.
                        None => true,
                    }
                })
                .map(|prevout| prevout.value.to_sat())
                .sum(),
        )
    }

    /// Computes the sum of all outputs, or only those whose script pubkey is in `filter`, if
    /// provided.
    fn _sum_outputs(&self, filter: Option<&HashSet<RScriptBuf>>) -> u64 {
        self.inner
            .output
            .iter()
            .filter(|txout| {
                match &filter {
                    Some(filter) => filter.contains(txout.script_pubkey.as_script()),
                    // No filter.
                    None => true,
                }
            })
            .map(|txout| txout.value.to_sat())
            .sum()
    }

    /// Computes the total value of all inputs. Returns `None` if any input is missing a previous
    /// output.
    pub fn sum_inputs(&self) -> SyncReturn<Option<u64>> {
        SyncReturn(self._sum_inputs(None))
    }

    /// Computes the sum of all outputs.
    pub fn sum_outputs(&self) -> SyncReturn<u64> {
        SyncReturn(self._sum_outputs(None))
    }

    /// Computes the total value of inputs we own. Returns `None` if any owned input is missing a
    /// previous output.
    pub fn sum_owned_inputs(&self) -> SyncReturn<Option<u64>> {
        SyncReturn(self._sum_inputs(Some(&self.is_mine)))
    }

    /// Computes the total value of outputs we own.
    pub fn sum_owned_outputs(&self) -> SyncReturn<u64> {
        SyncReturn(self._sum_outputs(Some(&self.is_mine)))
    }

    /// Computes the total value of inputs that spend a previous output with the given `spk`.
    ///
    /// Returns `None` if any input is missing a previous output.
    pub fn sum_inputs_spending_spk(&self, spk: RustOpaque<RScriptBuf>) -> SyncReturn<Option<u64>> {
        SyncReturn(self._sum_inputs(Some(&[spk.as_script().to_owned()].into())))
    }

    /// Computes the total value of outputs that send to the given script pubkey.
    pub fn sum_outputs_to_spk(&self, spk: RustOpaque<RScriptBuf>) -> SyncReturn<u64> {
        SyncReturn(self._sum_outputs(Some(&[spk.as_script().to_owned()].into())))
    }

    /// Computes the net change in our owned balance: owned outputs minus owned inputs.
    ///
    /// Returns `None` if any owned input is missing a previous output.
    pub fn balance_delta(&self) -> SyncReturn<Option<i64>> {
        SyncReturn((|| -> Option<i64> {
            let owned_inputs_sum: i64 = self
                ._sum_inputs(Some(&self.is_mine))?
                .try_into()
                .expect("net spent value must convert to i64");
            let owned_outputs_sum: i64 = self
                ._sum_outputs(Some(&self.is_mine))
                .try_into()
                .expect("net created value must convert to i64");
            Some(owned_outputs_sum.saturating_sub(owned_inputs_sum))
        })())
    }

    /// Computes the transaction fee as the difference between total input and output value.
    /// Returns `None` if any input is missing a previous output.
    pub fn fee(&self) -> SyncReturn<Option<u64>> {
        SyncReturn((|| -> Option<u64> {
            let inputs_sum = self._sum_inputs(None)?;
            let outputs_sum = self._sum_outputs(None);
            Some(outputs_sum.saturating_sub(inputs_sum))
        })())
    }

    pub fn timestamp(&self) -> SyncReturn<Option<u64>> {
        SyncReturn(
            self.confirmation_time
                .as_ref()
                .map(|t| t.time)
                .or(self.last_seen),
        )
    }

    /// Feerate in sats/vbyte.
    pub fn feerate(&self) -> SyncReturn<Option<f64>> {
        SyncReturn(|| -> Option<f64> {
            Some(((self.fee().0?) as f64) / (self.inner.vsize() as f64))
        }())
    }

    pub fn recipients(&self) -> SyncReturn<Vec<TxOutInfo>> {
        SyncReturn(
            self.inner
                .output
                .iter()
                .zip(0_u32..)
                .map(|(txout, vout)| TxOutInfo {
                    vout,
                    amount: txout.value.to_sat(),
                    script_pubkey: RustOpaque::new(txout.script_pubkey.clone()),
                    is_mine: self.is_mine.contains(&txout.script_pubkey),
                })
                .collect(),
        )
    }

    /// Return a transaction with the following signatures added.
    pub fn with_signatures(&self, signatures: Vec<EncodedSignature>) -> RustOpaque<RTransaction> {
        let mut tx = (*self.inner).clone();
        for (txin, signature) in tx.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                    .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.witness = witness;
        }
        RustOpaque::new(tx)
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
    pub recoverable: Vec<RecoverableKey>,
}

#[derive(Clone, Debug)]
pub struct RecoverableKey {
    pub name: String,
    pub threshold: u16,
    pub access_structure_ref: AccessStructureRef,
    pub shares_obtained: u16,
}

#[derive(Clone, Debug)]
pub struct FrostKey(pub(crate) RustOpaque<frostsnap_core::coordinator::CoordFrostKey>);

impl FrostKey {
    pub fn master_appkey(&self) -> SyncReturn<Option<MasterAppkey>> {
        SyncReturn(
            self.0
                .complete_key
                .as_ref()
                .map(|complete_key| complete_key.master_appkey),
        )
    }

    pub fn key_id(&self) -> SyncReturn<KeyId> {
        SyncReturn(self.0.key_id)
    }

    pub fn key_name(&self) -> SyncReturn<String> {
        SyncReturn(self.0.key_name.clone())
    }

    pub fn access_structures(&self) -> SyncReturn<Vec<AccessStructure>> {
        SyncReturn(
            self.0
                .complete_key
                .iter()
                .flat_map(|complete_key| {
                    complete_key
                        .access_structures
                        .values()
                        .cloned()
                        .map(From::from)
                })
                .collect(),
        )
    }

    pub fn is_complete(&self) -> SyncReturn<bool> {
        SyncReturn(self.0.complete_key.is_some())
    }

    pub fn access_structure_state(&self) -> SyncReturn<AccessStructureListState> {
        SyncReturn(AccessStructureListState::from_frost_key(&self.0))
    }

    pub fn bitcoin_network(&self) -> SyncReturn<Option<BitcoinNetwork>> {
        SyncReturn(
            self.0
                .purpose
                .bitcoin_network()
                .map(|network| network.into()),
        )
    }
}

#[derive(Clone, Debug)]
pub struct RecoveringAccessStructure {
    pub access_structure_id: AccessStructureId,
    pub threshold: u16,
    pub got_shares_from: Vec<DeviceId>,
}

impl RecoveringAccessStructure {
    fn from_core(
        access_structure_id: AccessStructureId,
        recovering_access_structure: frostsnap_core::coordinator::RecoveringAccessStructure,
    ) -> Self {
        RecoveringAccessStructure {
            access_structure_id,
            threshold: recovering_access_structure.threshold,
            got_shares_from: recovering_access_structure
                .share_images
                .iter()
                .map(|(_, (device_id, _))| *device_id)
                .collect::<HashSet<DeviceId>>()
                .into_iter()
                .collect(),
        }
    }
}

pub struct AccessStructureListState(pub Vec<AccessStructureState>);

pub enum AccessStructureState {
    Recovering(RecoveringAccessStructure),
    Complete(AccessStructure),
}

impl AccessStructureListState {
    pub(crate) fn from_frost_key(frost_key: &CoordFrostKey) -> Self {
        let complete = frost_key.access_structures().map(|access_structure| {
            AccessStructureState::Complete(AccessStructure::from(access_structure.clone()))
        });
        let recovering = frost_key.recovering_access_structures.iter().map(
            |(access_structure_id, recovering_access_structure)| {
                AccessStructureState::Recovering(RecoveringAccessStructure::from_core(
                    *access_structure_id,
                    recovering_access_structure.clone(),
                ))
            },
        );

        AccessStructureListState(complete.chain(recovering).collect())
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
    pub fn threshold(&self) -> SyncReturn<u16> {
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
pub struct SuperWallet {
    pub inner: RustOpaque<Mutex<CoordSuperWallet>>,
    pub wallet_streams: RustOpaque<WalletStreams>,
    pub chain_sync: RustOpaque<ChainClient>,
    pub network: BitcoinNetwork,
}

impl SuperWallet {
    fn load_or_new(
        app_dir: impl AsRef<Path>,
        network: BitcoinNetwork,
        chain_sync: ChainClient,
    ) -> Result<SuperWallet> {
        let db_file = network.bdk_file(app_dir);
        let db = rusqlite::Connection::open(&db_file).context(format!(
            "failed to load database from {}",
            db_file.display()
        ))?;

        let db = Arc::new(Mutex::new(db));

        let super_wallet =
            CoordSuperWallet::load_or_init(db.clone(), *network.0, chain_sync.clone())
                .with_context(|| format!("loading wallet from data in {}", db_file.display()))?;

        let wallet = SuperWallet {
            inner: RustOpaque::new(Mutex::new(super_wallet)),
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

    pub fn height(&self) -> SyncReturn<u32> {
        SyncReturn(self.inner.lock().unwrap().chain_tip().height())
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

    pub fn next_unused_address(&self, master_appkey: MasterAppkey) -> Result<Address> {
        self.inner
            .lock()
            .unwrap()
            .next_unused_address(master_appkey)
            .map(Into::into)
    }

    pub fn address_state(
        &self,
        master_appkey: MasterAppkey,
        index: u32,
    ) -> SyncReturn<Option<Address>> {
        SyncReturn(
            self.inner
                .lock()
                .unwrap()
                .address(master_appkey, index)
                .map(Into::into),
        )
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
            .map(|address_info| address_info.into())
    }

    pub fn mark_address_shared(
        &self,
        master_appkey: MasterAppkey,
        derivation_index: u32,
    ) -> Result<bool> {
        self.inner
            .lock()
            .unwrap()
            .mark_address_shared(master_appkey, derivation_index)
    }

    pub fn unmark_address_shared(
        &self,
        master_appkey: MasterAppkey,
        derivation_index: u32,
    ) -> Result<bool> {
        self.inner
            .lock()
            .unwrap()
            .unmark_address_shared(master_appkey, derivation_index)
    }

    pub fn rebroadcast(&self, txid: String) {
        let txid = Txid::from_str(&txid).expect("Txid must be valid");
        let super_wallet = self.inner.lock().unwrap();
        if let Some(tx) = super_wallet.get_tx(txid) {
            if let Err(err) = self.chain_sync.broadcast(tx.as_ref().clone()) {
                tracing::error!("Rebroadcasting {} failed: {}", txid, err);
            };
        }
    }

    /// Returns feerate in sat/vB.
    pub fn estimate_fee(&self, target_blocks: Vec<u64>) -> Result<Vec<(u64, u64)>> {
        let fee_rate_map = self
            .chain_sync
            .estimate_fee(target_blocks.into_iter().map(|v| v as usize))?;
        Ok(fee_rate_map
            .into_iter()
            .map(|(target, fee_rate)| (target as u64, fee_rate.to_sat_per_vb_ceil()))
            .collect())
    }

    pub fn send_to(
        &self,
        master_appkey: MasterAppkey,
        to_address: String,
        value: u64,
        feerate: f64,
    ) -> Result<UnsignedTx> {
        let mut super_wallet = self.inner.lock().unwrap();
        let to_address = bitcoin::Address::from_str(&to_address)
            .expect("validation should have checked")
            .require_network(super_wallet.network)
            .expect("validation should have checked");
        let signing_task =
            super_wallet.send_to(master_appkey, to_address, value, feerate as f32)?;
        let unsigned_tx = UnsignedTx {
            template_tx: RustOpaque::new(signing_task),
        };
        Ok(unsigned_tx)
    }

    pub fn calculate_avaliable(
        &self,
        master_appkey: MasterAppkey,
        target_addresses: Vec<String>,
        feerate: f64,
    ) -> Result<i64> {
        let mut wallet = self.inner.lock().unwrap();
        let network = wallet.network;
        wallet.calculate_avaliable_value(
            master_appkey,
            target_addresses.into_iter().map(|s| {
                bitcoin::Address::from_str(&s)
                    .expect("validation should have checked")
                    .require_network(network)
                    .expect("validation should have checked")
            }),
            feerate as f32,
            true,
        )
    }

    pub fn broadcast_tx(
        &self,
        master_appkey: MasterAppkey,
        tx: RustOpaque<RTransaction>,
    ) -> Result<()> {
        match self.chain_sync.broadcast((*tx).clone()) {
            Ok(_) => {
                event!(
                    Level::INFO,
                    tx = tx.compute_txid().to_string(),
                    "transaction successfully broadcast"
                );
                let mut inner = self.inner.lock().unwrap();
                inner.broadcast_success((*tx).to_owned());
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
                tx.consensus_encode(&mut buf).unwrap();
                let hex_tx = hex::encode(&buf);
                event!(
                    Level::ERROR,
                    tx = tx.compute_txid().to_string(),
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
}

pub fn load(app_dir: String) -> anyhow::Result<(Coordinator, AppCtx)> {
    let app_dir = PathBuf::from_str(&app_dir)?;
    let usb_manager = UsbSerialManager::new(Box::new(DesktopSerial), crate::FIRMWARE);
    _load(app_dir, usb_manager)
}

pub fn load_host_handles_serial(
    app_dir: String,
) -> anyhow::Result<(Coordinator, AppCtx, FfiSerial)> {
    let app_dir = PathBuf::from_str(&app_dir)?;
    let ffi_serial = FfiSerial::default();
    let usb_manager = UsbSerialManager::new(Box::new(ffi_serial.clone()), crate::FIRMWARE);
    let (coord, app_state) = _load(app_dir, usb_manager)?;
    Ok((coord, app_state, ffi_serial))
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

    // FIXME: doesn't need to be on the network. Can get the script pubkey without the network.
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
    let coordinator = Coordinator(RustOpaque::new(coordinator));
    let app_state = AppCtx {
        settings: Settings::new(db.clone(), app_dir)?,
        backup_manager: BackupManager::new(db.clone())?,
    };
    println!("loaded db");

    Ok((coordinator, app_state))
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

pub struct AppCtx {
    pub settings: Settings,
    pub backup_manager: BackupManager,
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
        SyncReturn(self.0.key_state())
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
                        .filter(|access_structure| access_structure.contains_device(device_id))
                        .map(|access_structure| access_structure.access_structure_ref())
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
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::Test { message },
            SinkWrap(sink),
        )?;
        Ok(())
    }

    pub fn start_signing_tx(
        &self,
        access_structure_ref: AccessStructureRef,
        unsigned_tx: UnsignedTx,
        devices: Vec<DeviceId>,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.deref().clone()),
            SinkWrap(sink),
        )?;
        Ok(())
    }

    pub fn nonces_available(&self, id: DeviceId) -> SyncReturn<u32> {
        SyncReturn(self.0.nonces_available(id))
    }

    pub fn generate_new_key(
        &self,
        threshold: u16,
        devices: Vec<DeviceId>,
        key_name: String,
        network: BitcoinNetwork,
        event_stream: StreamSink<KeyGenState>,
    ) -> anyhow::Result<()> {
        self.0.generate_new_key(
            devices.into_iter().collect(),
            threshold,
            key_name,
            KeyPurpose::Bitcoin(network.into()),
            event_stream,
        )
    }

    pub fn try_restore_signing_session(
        &self,
        session_id: SignSessionId,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0
            .try_restore_signing_session(session_id, SinkWrap(sink))
    }

    pub fn active_signing_session(
        &self,
        session_id: SignSessionId,
    ) -> SyncReturn<Option<ActiveSignSession>> {
        SyncReturn(
            self.0
                .inner()
                .active_signing_sessions_by_ssid()
                .get(&session_id)
                .cloned()
                .map(RustOpaque::new)
                .map(ActiveSignSession),
        )
    }

    pub fn active_signing_sessions(&self, key_id: KeyId) -> SyncReturn<Vec<ActiveSignSession>> {
        SyncReturn(
            self.0
                .inner()
                .active_signing_sessions()
                .filter(|session| session.key_id == key_id)
                .map(RustOpaque::new)
                .map(ActiveSignSession)
                .collect(),
        )
    }

    pub fn unbroadcasted_txs(
        &self,
        super_wallet: SuperWallet,
        key_id: KeyId,
    ) -> SyncReturn<Vec<SignedTxDetails>> {
        let coord = self.0.inner();
        let super_wallet = &*super_wallet.inner.lock().unwrap();
        let txs = coord
            .finished_signing_sessions()
            .iter()
            .filter(|(_, session)| session.key_id == key_id)
            .inspect(|&(&id, _)| event!(Level::DEBUG, "Found finished signing session: {}", id))
            .filter_map(|(_, session)| match &session.init.group_request.sign_task {
                WireSignTask::BitcoinTransaction(tx_temp) => {
                    let mut raw_tx = tx_temp.to_rust_bitcoin_tx();
                    let txid = raw_tx.compute_txid();
                    // Filter out txs that are already broadcasted.
                    if super_wallet.get_tx(txid).is_some() {
                        return None;
                    }
                    for (txin, signature) in raw_tx.input.iter_mut().zip(&session.signatures) {
                        let schnorr_sig = bitcoin::taproot::Signature {
                            signature: bitcoin::secp256k1::schnorr::Signature::from_slice(
                                &signature.0,
                            )
                            .unwrap(),
                            sighash_type: bitcoin::sighash::TapSighashType::Default,
                        };
                        let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
                        txin.witness = witness;
                    }
                    let is_mine = tx_temp
                        .iter_locally_owned_inputs()
                        .map(|(_, _, spk)| spk.spk())
                        .chain(
                            tx_temp
                                .iter_locally_owned_outputs()
                                .map(|(_, _, spk)| spk.spk()),
                        )
                        .collect::<HashSet<RScriptBuf>>();
                    let prevouts = tx_temp
                        .inputs()
                        .iter()
                        .map(|input| (input.outpoint(), input.txout()))
                        .collect::<HashMap<bitcoin::OutPoint, bitcoin::TxOut>>();
                    Some(SignedTxDetails {
                        session_id: session.init.group_request.session_id(),
                        tx: Transaction {
                            inner: RustOpaque::new(raw_tx),
                            txid: txid.to_string(),
                            confirmation_time: None,
                            last_seen: None,
                            prevouts: RustOpaque::new(prevouts),
                            is_mine: RustOpaque::new(is_mine),
                        },
                    })
                }
                _ => None,
            });
        SyncReturn(txs.collect())
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
        key_id: KeyId,
        address_index: u32,
        sink: StreamSink<VerifyAddressProtocolState>,
    ) -> Result<()> {
        self.0.verify_address(key_id, address_index, sink)?;
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

    pub fn final_keygen_ack(&self, keygen_id: KeygenId) -> Result<AccessStructureRef> {
        self.0.final_keygen_ack(keygen_id)
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

    pub fn request_device_sign(
        &self,
        device_id: DeviceId,
        session_id: SignSessionId,
    ) -> Result<()> {
        self.0
            .request_device_sign(device_id, session_id, crate::TEMP_KEY)
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

    pub fn start_recovery(&self, key_id: KeyId) -> Result<()> {
        self.0.start_recovery(key_id)?;

        Ok(())
    }

    pub fn delete_key(&self, key_id: KeyId) -> Result<()> {
        self.0.delete_key(key_id)
    }

    pub fn device_at_index(&self, index: usize) -> SyncReturn<Option<ConnectedDevice>> {
        SyncReturn(self.0.device_at_index(index))
    }

    pub fn device_list_state(&self) -> SyncReturn<DeviceListState> {
        SyncReturn(self.0.device_list_state())
    }

    pub fn sub_device_events(&self, sink: StreamSink<DeviceListUpdate>) -> Result<()> {
        self.0.sub_device_events(sink);
        Ok(())
    }

    pub fn get_connected_device(&self, id: DeviceId) -> SyncReturn<Option<ConnectedDevice>> {
        SyncReturn(self.0.get_connected_device(id))
    }

    pub fn wipe_device_data(&self, device_id: DeviceId) {
        self.0.wipe_device_data(device_id);
    }

    pub fn cancel_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        self.0.cancel_sign_session(ssid)
    }

    pub fn forget_finished_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        self.0.forget_finished_sign_session(ssid)
    }

    pub fn sub_signing_session_signals(&self, key_id: KeyId, sink: StreamSink<()>) {
        self.0.sub_signing_session_signals(key_id, SinkWrap(sink))
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
    pub fn txid(&self) -> SyncReturn<String> {
        SyncReturn(self.signed_tx.compute_txid().to_string())
    }

    pub fn effect(
        &self,
        master_appkey: MasterAppkey,
        network: BitcoinNetwork,
    ) -> Result<SyncReturn<EffectOfTx>> {
        self.unsigned_tx.effect(master_appkey, network)
    }
}

impl UnsignedTx {
    pub fn txid(&self) -> SyncReturn<String> {
        SyncReturn(self.template_tx.txid().to_string())
    }

    pub fn fee(&self) -> SyncReturn<Option<u64>> {
        SyncReturn(self.template_tx.fee())
    }

    pub fn feerate(&self) -> SyncReturn<Option<f64>> {
        SyncReturn(self.template_tx.feerate())
    }

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

    pub fn details(
        &self,
        super_wallet: SuperWallet,
        master_appkey: MasterAppkey,
    ) -> SyncReturn<Transaction> {
        let super_wallet = super_wallet.inner.lock().unwrap();
        let tx_temp = &*self.template_tx;
        let raw_tx = tx_temp.to_rust_bitcoin_tx();
        let txid = raw_tx.compute_txid();
        SyncReturn(Transaction {
            txid: txid.to_string(),
            confirmation_time: None,
            last_seen: None,
            prevouts: RustOpaque::new(
                super_wallet.get_prevouts(raw_tx.input.iter().map(|txin| txin.previous_output)),
            ),
            is_mine: RustOpaque::new(
                raw_tx
                    .output
                    .iter()
                    .chain(
                        super_wallet
                            .get_prevouts(raw_tx.input.iter().map(|txin| txin.previous_output))
                            .values(),
                    )
                    .map(|txout| txout.script_pubkey.clone())
                    .filter(|spk| super_wallet.is_spk_mine(master_appkey, spk.clone()))
                    .collect::<HashSet<RScriptBuf>>(),
            ),
            inner: RustOpaque::new(raw_tx),
        })
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
    pub inner: RustOpaque<RAddress<RNetworkChecked>>,
    pub index: u32,
    pub used: bool,
    pub shared: bool,
    pub fresh: bool,
    pub is_external: bool,
    pub derivation_path: String,
}

impl From<frostsnap_coordinator::bitcoin::wallet::AddressInfo> for Address {
    fn from(value: frostsnap_coordinator::bitcoin::wallet::AddressInfo) -> Self {
        let address = Self {
            inner: RustOpaque::new(value.address),
            index: value.index,
            used: value.used,
            shared: value.shared,
            fresh: value.fresh,
            is_external: value.external,
            derivation_path: value
                .derivation_path
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("/"),
        };
        address
    }
}

impl Address {
    pub fn address(&self) -> SyncReturn<String> {
        SyncReturn(self.inner.to_string())
    }

    pub fn spk(&self) -> SyncReturn<RustOpaque<RScriptBuf>> {
        SyncReturn(RustOpaque::new(self.inner.script_pubkey()))
    }
}

pub struct TxState {
    pub txs: Vec<Transaction>,
    pub balance: i64,
    pub untrusted_pending_balance: i64,
}

impl From<Vec<frostsnap_coordinator::bitcoin::wallet::Transaction>> for TxState {
    fn from(txs: Vec<frostsnap_coordinator::bitcoin::wallet::Transaction>) -> Self {
        let txs = txs
            .into_iter()
            .map(From::from)
            .collect::<Vec<Transaction>>();

        let mut balance = 0_i64;
        let mut untrusted_pending_balance = 0_i64;

        for tx in &txs {
            let filter = Some(&*tx.is_mine);
            let net_spent: i64 = tx
                ._sum_inputs(filter)
                .unwrap_or(0)
                .try_into()
                .expect("spent value must fit into i64");
            let net_created: i64 = tx
                ._sum_outputs(filter)
                .try_into()
                .expect("created value must fit into i64");
            if net_spent == 0 && tx.confirmation_time.is_none() {
                untrusted_pending_balance += net_created;
            } else {
                balance += net_created;
                balance -= net_spent;
            }
        }

        // Workaround as we are too lazy to exclude spends from unconfirmed as
        // `untrusted_pending_balance`.
        if balance < 0 {
            untrusted_pending_balance += balance;
            balance = 0;
        }

        Self {
            balance,
            untrusted_pending_balance,
            txs,
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

// pub enum SignTaskDescription {
//     Plain { message: String },
//     // Nostr {
//     //     #[bincode(with_serde)]
//     //     event: Box<crate::nostr::UnsignedEvent>,
//     //     master_appkey: MasterAppkey,
//     // }, // 1 nonce & sig
//     Transaction { unsigned_tx: UnsignedTx },
// }

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

    pub fn find_address_from_bytes(&self, bytes: Vec<u8>) -> Result<Option<String>> {
        let decoded_qr = crate::camera::read_qr_code_bytes(&bytes)?;
        for maybe_addr in decoded_qr {
            match bitcoin::Address::from_str(&maybe_addr) {
                Ok(_) => return Ok(Some(maybe_addr)),
                Err(_) => continue,
            }
        }
        Ok(None)
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
    pub loaded_wallets: RustOpaque<Mutex<HashMap<RBitcoinNetwork, SuperWallet>>>,

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

        let mut loaded_wallets: HashMap<RBitcoinNetwork, SuperWallet> = Default::default();
        let mut chain_apis = HashMap::new();

        for network in SUPPORTED_NETWORKS {
            let bitcoin_network = BitcoinNetwork::from(network);
            let electrum_url = persisted.get_electrum_server(network);
            let (chain_api, conn_handler) = ChainClient::new();
            let super_wallet =
                SuperWallet::load_or_new(&app_directory, bitcoin_network, chain_api.clone())?;
            conn_handler.run(electrum_url, super_wallet.inner.clone(), {
                let wallet_streams = super_wallet.wallet_streams.clone();
                move |master_appkey, txs| {
                    let wallet_streams = wallet_streams.lock().unwrap();
                    if let Some(stream) = wallet_streams.get(&master_appkey) {
                        stream.add(txs.into());
                    }
                }
            });
            loaded_wallets.insert(network, super_wallet);
            chain_apis.insert(network, chain_api);
        }

        Ok(Self {
            loaded_wallets: RustOpaque::new(Mutex::new(loaded_wallets)),
            settings: RustOpaque::new(Mutex::new(persisted)),
            app_directory: RustOpaque::new(app_directory),
            chain_clients: RustOpaque::new(chain_apis),
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

    pub fn get_super_wallet(&self, network: BitcoinNetwork) -> Result<SyncReturn<SuperWallet>> {
        let loaded = self.loaded_wallets.lock().unwrap();
        loaded
            .get(&network.0)
            .cloned()
            .map(SyncReturn)
            .ok_or(anyhow!("unsupported network {:?}", network.0))
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

pub struct BackupManager {
    pub backup_state: RustOpaque<Mutex<Persisted<BackupState>>>,
    pub backup_run_streams: RustOpaque<Mutex<BTreeMap<KeyId, StreamSink<BackupRun>>>>,
    pub db: RustOpaque<Arc<Mutex<rusqlite::Connection>>>,
}

impl BackupManager {
    fn new(db: Arc<Mutex<rusqlite::Connection>>) -> anyhow::Result<Self> {
        let persisted_backup_state: Persisted<BackupState> = {
            let mut db_ = db.lock().unwrap();
            Persisted::new(&mut *db_, ())?
        };

        Ok(Self {
            db: RustOpaque::new(db),
            backup_state: RustOpaque::new(Mutex::new(persisted_backup_state)),
            backup_run_streams: RustOpaque::new(Mutex::new(Default::default())),
        })
    }

    pub fn maybe_start_backup_run(&self, access_structure: AccessStructure) -> Result<()> {
        let key_id = access_structure.master_appkey().0.key_id();
        let mut db = self.db.lock().unwrap();
        self.backup_state
            .lock()
            .unwrap()
            .mutate2(&mut *db, |state, mutations| {
                let start_backup = !state.runs.contains_key(&key_id);
                if start_backup {
                    let devices = access_structure.devices().0;
                    state.runs.insert(key_id, BackupRun::new(devices.clone()));
                    mutations.push(BackupMutation::StartBackup { key_id, devices });
                }

                Ok(())
            })?;
        Ok(())
    }

    pub fn mark_backup_complete(&self, key_id: KeyId, device_id: DeviceId) -> Result<()> {
        {
            let mut db = self.db.lock().unwrap();
            self.backup_state
                .lock()
                .unwrap()
                .mutate2(&mut *db, |state, mutations| {
                    if let Some(run) = state.runs.get_mut(&key_id) {
                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as u32;

                        run.mark_device_complete(device_id);

                        mutations.push(BackupMutation::MarkDeviceComplete {
                            key_id,
                            device_id,
                            timestamp,
                        });
                    }

                    Ok(())
                })?;
        }
        self.backup_stream_emit(key_id)?;
        Ok(())
    }

    pub fn is_device_complete(&self, key_id: KeyId, device_id: DeviceId) -> SyncReturn<bool> {
        let backup_state = self.backup_state.lock().unwrap();
        let complete = backup_state
            .runs
            .get(&key_id)
            .map(|run| {
                run.devices
                    .iter()
                    .any(|(d, timestamp)| *d == device_id && timestamp.is_some())
            })
            .unwrap_or(false);
        SyncReturn(complete)
    }

    pub fn get_backup_run(&self, key_id: KeyId) -> SyncReturn<Option<BackupRun>> {
        SyncReturn(self.backup_state.lock().unwrap().runs.get(&key_id).cloned())
    }

    pub fn is_run_complete(&self, key_id: KeyId) -> SyncReturn<bool> {
        let complete = self
            .backup_state
            .lock()
            .unwrap()
            .runs
            .get(&key_id)
            .cloned()
            .map(|run| run.is_run_complete())
            .unwrap_or(false);
        SyncReturn(complete)
    }

    pub fn backup_stream(&self, key_id: KeyId, new_stream: StreamSink<BackupRun>) -> Result<()> {
        {
            let mut streams = self.backup_run_streams.lock().unwrap();
            streams.insert(key_id, new_stream);
        }
        self.backup_stream_emit(key_id)?;
        Ok(())
    }

    pub fn backup_stream_emit(&self, key_id: KeyId) -> Result<()> {
        let streams = self.backup_run_streams.lock().unwrap();
        let stream = match streams.get(&key_id) {
            Some(stream) => stream,
            None => return Err(anyhow!("no backup stream found for key: {}", key_id)),
        };

        let backup_state = self.backup_state.lock().unwrap();
        let run = backup_state
            .runs
            .get(&key_id)
            .ok_or_else(|| anyhow!("no backup run found for key: {}", key_id))?;

        let run_to_emit = run.clone();
        stream.add(run_to_emit);
        Ok(())
    }

    pub fn should_quick_backup_warn(&self, key_id: KeyId, device_id: DeviceId) -> SyncReturn<bool> {
        let too_fast_warning_period = 5 * 60;
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

        let backup_state = self.backup_state.lock().unwrap();
        let backup_run = backup_state.runs.get(&key_id);

        let most_recent = backup_run.and_then(|run| {
            run.devices
                .iter()
                .filter_map(|(dev_id, time)| time.map(|t| (*dev_id, t)))
                .max_by_key(|&(_, time)| time)
        });

        match most_recent {
            Some((last_device_id, timestamp)) => {
                let elapsed = current_time - timestamp;
                SyncReturn(elapsed < too_fast_warning_period && last_device_id != device_id)
            }
            None => SyncReturn(false),
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

#[frb(mirror(BackupRun))]
pub struct _BackupRun {
    pub devices: Vec<(DeviceId, Option<u32>)>,
}
#[frb(mirror(KeygenId))]
pub struct _KeygenId(pub [u8; 16]);

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

#[frb(mirror(SignSessionId))]
pub struct _SignSessionId(pub [u8; 32]);

#[frb(mirror(SigningState))]
pub struct _SigningState {
    pub session_id: SignSessionId,
    pub got_shares: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    // for some reason FRB woudln't allow Option here to empty vec implies not being finished
    pub finished_signatures: Vec<EncodedSignature>,
    pub aborted: Option<String>,
    pub connected_but_need_request: Vec<DeviceId>,
}

#[derive(Debug, Clone)]
pub enum SigningDetails {
    Message {
        message: String,
    },
    Transaction {
        transaction: Transaction,
    },
    Nostr {
        id: String,
        content: String,
        hash_bytes: String,
    },
}

pub struct ActiveSignSession(pub RustOpaque<RActiveSignSession>);

impl ActiveSignSession {
    pub fn state(&self) -> SyncReturn<SigningState> {
        let session_id = self.0.session_id();
        let session_init = &self.0.init;
        let got_shares = self.0.received_from();
        let state = SigningState {
            session_id,
            got_shares: got_shares.into_iter().collect(),
            needed_from: session_init.nonces.keys().copied().collect(),
            finished_signatures: Vec::new(),
            aborted: None,
            connected_but_need_request: Default::default(),
        };

        SyncReturn(state)
    }
    pub fn details(&self) -> SyncReturn<SigningDetails> {
        let session_init = &self.0.init;

        let res = match session_init.group_request.sign_task.clone() {
            WireSignTask::Test { message } => SigningDetails::Message { message },
            WireSignTask::Nostr { event } => SigningDetails::Nostr {
                id: event.id,
                content: event.content,
                hash_bytes: event.hash_bytes.to_lower_hex_string(),
            },
            WireSignTask::BitcoinTransaction(tx_temp) => {
                let raw_tx = tx_temp.to_rust_bitcoin_tx();
                let txid = raw_tx.compute_txid();
                let is_mine = tx_temp
                    .iter_locally_owned_inputs()
                    .map(|(_, _, spk)| spk.spk())
                    .chain(
                        tx_temp
                            .iter_locally_owned_outputs()
                            .map(|(_, _, spk)| spk.spk()),
                    )
                    .collect::<HashSet<RScriptBuf>>();
                let prevouts = tx_temp
                    .inputs()
                    .iter()
                    .map(|input| (input.outpoint(), input.txout()))
                    .collect::<HashMap<bitcoin::OutPoint, bitcoin::TxOut>>();
                SigningDetails::Transaction {
                    transaction: Transaction {
                        inner: RustOpaque::new(raw_tx),
                        txid: txid.to_string(),
                        confirmation_time: None,
                        last_seen: None,
                        prevouts: RustOpaque::new(prevouts),
                        is_mine: RustOpaque::new(is_mine),
                    },
                }
            }
        };
        SyncReturn(res)
    }
}

#[derive(Debug, Clone)]
pub struct SignedTxDetails {
    pub session_id: SignSessionId,
    pub tx: Transaction,
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
    pub keygen_id: KeygenId,
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

pub fn echo_device_list_update(value: DeviceListUpdate) -> DeviceListUpdate {
    value
}

// In flutter_rust_bridge v2 we can just extend MasterAppkey with this
pub fn master_appkey_ext_to_key_id(master_appkey: MasterAppkey) -> SyncReturn<KeyId> {
    SyncReturn(master_appkey.key_id())
}
