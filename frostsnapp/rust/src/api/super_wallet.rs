use super::bitcoin::BitcoinNetworkExt as _;
use super::coordinator::Coordinator;
use super::transaction::BuildTxState;
use super::{bitcoin::Transaction, signing::UnsignedTx};
use crate::api::broadcast::Broadcast;
use crate::api::psbt_manager::PsbtManager;
use crate::coordinator::FfiCoordinator;
use crate::frb_generated::{RustAutoOpaque, StreamSink};
use crate::sink_wrap::SinkWrap;
use anyhow::{Context as _, Result};
use bitcoin::Transaction as RTransaction;
use bitcoin::Txid;
pub use bitcoin::{Address, Network as BitcoinNetwork, Psbt};
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::bitcoin::wallet::AddressInfo;
pub use frostsnap_coordinator::bitcoin::wallet::PsbtValidationError;
pub use frostsnap_coordinator::bitcoin::{chain_sync::ChainClient, wallet::CoordSuperWallet};
use frostsnap_coordinator::persist::Persisted;
pub use frostsnap_coordinator::verify_address::VerifyAddressProtocolState;

use frostsnap_core::coordinator::{ActiveSignSession, FinishedSignSession, FrostCoordinator};
use frostsnap_core::{DeviceId, KeyId, MasterAppkey};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::RwLock;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};

type WalletSignals = BTreeMap<MasterAppkey, Broadcast<()>>;

pub struct WalletBalance {
    pub balance: i64,
    pub untrusted_pending_balance: i64,
}

pub struct TxState {
    inner_wallet: Arc<Mutex<CoordSuperWallet>>,
    master_appkey: MasterAppkey,

    // This only updates based on changes to bitcoin wallet.
    //
    // This does not react to the coordinator.
    signal: Broadcast<()>,
    // pub txs: Vec<Transaction>,
    // pub balance: i64,
    // pub untrusted_pending_balance: i64,
}

#[frb(external)]
impl PsbtValidationError {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}

#[derive(Clone)]
#[frb(opaque)]
pub struct SuperWallet {
    pub(crate) coord_inner: Arc<Mutex<Persisted<FrostCoordinator>>>,
    pub(crate) signing_session_broadcasts: Arc<Mutex<HashMap<KeyId, Broadcast<()>>>>,
    pub(crate) psbt_man: PsbtManager,
    pub(crate) inner: Arc<Mutex<CoordSuperWallet>>,
    pub(crate) wallet_signals: Arc<Mutex<WalletSignals>>,
    chain_sync: ChainClient,
    pub(crate) network: BitcoinNetwork,
}

impl SuperWallet {
    #[allow(unused)]
    pub(crate) fn load_or_new(
        coord: &Coordinator,
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

        let super_wallet = CoordSuperWallet::load_or_init(db.clone(), network, chain_sync.clone())
            .with_context(|| format!("loading wallet from data in {}", db_file.display()))?;

        let psbt_man = PsbtManager::new(db);

        let wallet = SuperWallet {
            coord_inner: coord.0.clone_inner(),
            signing_session_broadcasts: coord.0.signing_session_broadcasts(),
            psbt_man,
            inner: Arc::new(Mutex::new(super_wallet)),
            chain_sync,
            wallet_signals: Default::default(),
            network,
        };

        Ok(wallet)
    }

    pub(crate) fn notify_wallet(&self, master_appkey: MasterAppkey) {
        let signal_streams = self.wallet_signals.lock().unwrap();
        if let Some(stream) = signal_streams.get(&master_appkey) {
            stream.add(&());
        }
    }

    #[frb(sync)]
    pub fn height(&self) -> u32 {
        self.inner.lock().unwrap().chain_tip().height()
    }

    fn _active_tx_ss<'a>(
        coord: &'a Persisted<FrostCoordinator>,
        key_id: KeyId,
    ) -> impl Iterator<Item = (Txid, ActiveSignSession)> + 'a {
        coord
            .active_signing_sessions()
            .filter(|ss| ss.key_id == key_id)
            .filter_map(|ss| match &ss.init.group_request.sign_task {
                frostsnap_core::WireSignTask::BitcoinTransaction(temp) => Some((temp.txid(), ss)),
                _ => None,
            })
    }

    fn _finished_tx_ss<'a>(
        coord: &'a Persisted<FrostCoordinator>,
        key_id: KeyId,
    ) -> impl Iterator<Item = (Txid, FinishedSignSession)> + 'a {
        coord.finished_signing_sessions_by_ssid().values();
    }

    #[frb(sync)]
    pub fn relevant_txs(&self, master_appkey: MasterAppkey) -> Vec<Transaction> {
        let key_id = master_appkey.key_id();

        // So we need a list of txids, and also a map of txid -> transaction.
        let mut relevant_order = Vec::<Txid>::new();
        let mut relevant_txs = HashMap::<Txid, Transaction>::new();

        let inner = self.inner.lock().unwrap();
        for wallet_tx in inner.relevant_txs(master_appkey) {
            relevant_order.push(tx.txid);
            relevant_txs.insert(
                tx.txid,
                Transaction::from_wallet_tx(inner.indexed_tx_graph(), wallet_tx),
            );
        }
        drop(inner);

        // Get signing sessions
        let coord_inner = self.coord_inner.lock().unwrap();

        let active_ss =
            Self::_active_tx_ss(&*coord_inner, key_id).collect::<Vec<(Txid, ActiveSignSession)>>();
        let active_ss_map = active_ss
            .iter()
            .cloned()
            .collect::<HashMap<Txid, ActiveSignSession>>();

        let finished_ss = coord_inner
            .finished_signing_sessions_by_ssid()
            .values()
            .filter(|ss| ss.key_id == key_id);

        drop(coord_inner);

        // Fill relevant_order/txs with signing sessions.
        // If the txid does not exist - create it!

        // Fill relevant_order/txs with psbts.
    }

    #[frb(sync)]
    pub fn tx_state(&self, master_appkey: MasterAppkey) -> TxState {
        let inner = self.inner.lock().unwrap();
        let txs = inner.relevant_txs(master_appkey);

        let txs = self.inner.lock().unwrap().list_transactions(master_appkey);
        txs.into()
    }

    pub fn reconnect(&self) {
        self.chain_sync.reconnect();
    }

    #[frb(sync)]
    pub fn next_address(&self, master_appkey: MasterAppkey) -> AddressInfo {
        self.inner
            .lock()
            .unwrap()
            .next_address(master_appkey)
            .into()
    }
    #[frb(sync)]
    pub fn get_address_info(&self, master_appkey: MasterAppkey, index: u32) -> Option<AddressInfo> {
        self.inner.lock().unwrap().address(master_appkey, index)
    }

    #[frb(sync)]
    pub fn addresses_state(&self, master_appkey: MasterAppkey) -> Vec<AddressInfo> {
        self.inner.lock().unwrap().list_addresses(master_appkey)
    }

    #[frb(sync)]
    pub fn test_address_info() -> AddressInfo {
        AddressInfo {
            index: 24,
            address: bitcoin::Address::from_str(
                "bc1pp7w6kxnj7lzgm29pmuhezwl0vjdlcrthqukll5gn9xuqfq5n673smy4m63",
            )
            .unwrap()
            .assume_checked(),
            external: true,
            used: false,
            revealed: false,
            derivation_path: vec![],
        }
    }

    pub fn search_for_address(
        &self,
        master_appkey: MasterAppkey,
        address_str: String,
        start: u32,
        stop: u32,
    ) -> Option<AddressInfo> {
        self.inner
            .lock()
            .unwrap()
            .search_for_address(master_appkey, address_str, start, stop)
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

    pub fn rebroadcast(&self, txid: String) -> Result<()> {
        let txid = Txid::from_str(&txid).expect("Txid must be valid");
        let wallet = self.inner.lock().unwrap();
        let tx = wallet
            .get_tx(txid)
            .ok_or(anyhow::anyhow!("Transaction {txid} does not exist"))?;
        drop(wallet);

        self.chain_sync
            .broadcast(tx.as_ref().clone())
            .context("Rebroadcasting failed")?;
        Ok(())
    }

    /// Returns feerate in sat/vB.
    #[frb(type_64bit_int)]
    pub fn estimate_fee(&self, target_blocks: Vec<u64>) -> Result<Vec<(u64, u64)>> {
        let fee_rate_map = self
            .chain_sync
            .estimate_fee(target_blocks.into_iter().map(|v| v as usize))?;
        Ok(fee_rate_map
            .into_iter()
            .map(|(target, fee_rate)| (target as u64, fee_rate.to_sat_per_vb_ceil()))
            .collect())
    }

    /// Returns balance of the given `master_appkey`.
    pub fn balance(&self, master_appkey: MasterAppkey) -> WalletBalance {
        let txs = self.relevant_txs(master_appkey);
        todo!()
    }

    #[frb(type_64bit_int)]
    pub fn send_to(
        &self,
        master_appkey: MasterAppkey,
        to_address: &Address,
        value: u64,
        feerate: f64,
    ) -> Result<UnsignedTx> {
        let mut super_wallet = self.inner.lock().unwrap();
        let signing_task = super_wallet.send_to(
            master_appkey,
            [(to_address.clone(), Some(value))],
            feerate as f32,
        )?;
        let unsigned_tx = UnsignedTx {
            template_tx: signing_task,
        };
        Ok(unsigned_tx)
    }

    #[frb(sync)]
    pub fn calculate_available(
        &self,
        master_appkey: MasterAppkey,
        target_addresses: Vec<RustAutoOpaque<Address>>,
        feerate: f32,
    ) -> i64 {
        let mut wallet = self.inner.lock().unwrap();
        wallet.calculate_avaliable_value(
            master_appkey,
            target_addresses
                .into_iter()
                .map(|a| a.blocking_read().clone()),
            feerate,
            true,
        )
    }

    /// Start building transaction.
    ///
    /// Returns `None` if wallet under `master_appkey` is incomplete.
    #[frb(sync)]
    pub fn build_tx(
        &self,
        coord: RustAutoOpaque<Coordinator>,
        master_appkey: MasterAppkey,
    ) -> Option<BuildTxState> {
        let frost_key = coord
            .blocking_read()
            .get_frost_key(master_appkey.key_id())?;
        let state = BuildTxState {
            coord,
            super_wallet: self.clone(),
            frost_key,
            broadcast: Broadcast::default(),
            is_refreshing: Arc::new(AtomicBool::new(false)),
            inner: Arc::new(RwLock::new(super::transaction::BuildTxInner {
                confirmation_estimates: None,
                confirmation_target: super::transaction::ConfirmationTarget::default(),
                recipients: Vec::new(),
                access_id: None,
                signers: HashSet::new(),
            })),
        };
        Some(state)
    }

    pub fn broadcast_tx(&self, master_appkey: MasterAppkey, tx: &RTransaction) -> Result<()> {
        match self.chain_sync.broadcast(tx.clone()) {
            Ok(_) => {
                event!(
                    Level::INFO,
                    tx = tx.compute_txid().to_string(),
                    "transaction successfully broadcast"
                );
                let mut inner = self.inner.lock().unwrap();
                inner.broadcast_success(tx);
                let wallet_signals = self.wallet_signals.lock().unwrap();
                if let Some(stream) = wallet_signals.get(&master_appkey) {
                    stream.add(&());
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
        psbt: &Psbt,
        master_appkey: MasterAppkey,
    ) -> Result<UnsignedTx, PsbtValidationError> {
        let template = self
            .inner
            .lock()
            .unwrap()
            .psbt_to_tx_template(psbt, master_appkey)?;

        Ok(UnsignedTx {
            template_tx: template,
        })
    }
}

#[frb(mirror(AddressInfo), unignore, opaque)]
pub struct _AddressInfo {
    pub index: u32,
    pub address: bitcoin::Address,
    pub external: bool,
    pub used: bool,
    pub revealed: bool,
    pub derivation_path: Vec<u32>,
}

#[frb(mirror(VerifyAddressProtocolState), unignore)]
pub struct _VerifyAddressProtocolState {
    pub target_devices: Vec<DeviceId>,
}

impl super::coordinator::Coordinator {
    pub fn verify_address(
        &self,
        key_id: KeyId,
        address_index: u32,
        sink: StreamSink<VerifyAddressProtocolState>,
    ) -> Result<()> {
        self.0
            .verify_address(key_id, address_index, SinkWrap(sink))?;
        Ok(())
    }
}
