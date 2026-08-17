//! Shared fixtures for wallet-level tests: an in-memory wallet, a disconnected chain client, and
//! hand-built sync updates delivered through the same `apply_update` path real syncs use.

use super::chain_sync::{ChainClient, ConnectionHandler, ElectrumConfig};
use super::wallet::{CoordSuperWallet, KeychainId};
use crate::persist::Persisted;
use crate::settings::ElectrumEnabled;
use bdk_chain::bitcoin::{self, hashes::Hash, Amount, BlockHash, OutPoint, ScriptBuf, TxOut, Txid};
use bdk_chain::{BlockId, CanonicalizationParams, CheckPoint, ConfirmationBlockTime, TxUpdate};
use frostsnap_core::schnorr_fun::fun::Point;
use frostsnap_core::tweak::{BitcoinAccountKeychain, BitcoinBip32Path};
use frostsnap_core::MasterAppkey;
use std::sync::{Arc, Mutex};

pub(crate) const NETWORK: bitcoin::Network = bitcoin::Network::Bitcoin;

pub(crate) const EXTERNAL: fn() -> BitcoinAccountKeychain = BitcoinAccountKeychain::external;
pub(crate) const INTERNAL: fn() -> BitcoinAccountKeychain = BitcoinAccountKeychain::internal;

pub(crate) fn test_db() -> Arc<Mutex<rusqlite::Connection>> {
    Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()))
}

/// The handler owns the receiving ends of the client's channels, so it has to outlive every
/// `ChainClient` call or `monitor_keychain`'s send panics.
pub(crate) fn test_chain_client(
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> (ChainClient, ConnectionHandler) {
    let trusted_certificates = {
        let mut conn = db.lock().unwrap();
        Persisted::new(&mut *conn, NETWORK).unwrap()
    };
    ChainClient::new(
        bitcoin::constants::genesis_block(NETWORK).block_hash(),
        ElectrumConfig {
            enabled: ElectrumEnabled::None,
            primary: String::new(),
            backup: String::new(),
        },
        trusted_certificates,
        db.clone(),
    )
}

pub(crate) struct Fixture {
    pub(crate) db: Arc<Mutex<rusqlite::Connection>>,
    pub(crate) chain_client: ChainClient,
    _handler: ConnectionHandler,
    pub(crate) master_appkey: MasterAppkey,
    blocks: std::cell::RefCell<Vec<BlockId>>,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let db = test_db();
        let (chain_client, _handler) = test_chain_client(&db);
        Self {
            db,
            chain_client,
            _handler,
            master_appkey: MasterAppkey::derive_from_rootkey(
                Point::random(&mut rand::thread_rng()),
            ),
            blocks: std::cell::RefCell::new(vec![BlockId {
                height: 0,
                hash: bitcoin::constants::genesis_block(NETWORK).block_hash(),
            }]),
        }
    }

    pub(crate) fn wallet(&self) -> CoordSuperWallet {
        let mut wallet =
            CoordSuperWallet::load_or_init(self.db.clone(), NETWORK, self.chain_client.clone())
                .unwrap();
        wallet.list_addresses(self.master_appkey);
        wallet
    }

    pub(crate) fn spk(&self, account_keychain: BitcoinAccountKeychain, index: u32) -> ScriptBuf {
        super::peek_spk(
            self.master_appkey,
            BitcoinBip32Path {
                account_keychain,
                index,
            },
        )
    }

    /// What a real sync would deliver for `txs` confirmed at `height`: anchors, a connectable
    /// chain update, and the keychain activity the server's notification would have attributed.
    pub(crate) fn update(
        &self,
        txs: Vec<bitcoin::Transaction>,
        height: u32,
        last_active: Option<(BitcoinAccountKeychain, u32)>,
    ) -> bdk_electrum_streaming::Update<KeychainId> {
        let block_id = BlockId {
            height,
            hash: BlockHash::from_byte_array([height as u8; 32]),
        };
        let anchor = ConfirmationBlockTime {
            block_id,
            confirmation_time: 1_700_000_000 + height as u64,
        };
        let mut tx_update = TxUpdate::default();
        tx_update.anchors = txs.iter().map(|tx| (anchor, tx.compute_txid())).collect();
        tx_update.txs = txs.into_iter().map(Arc::new).collect();
        self.blocks.borrow_mut().push(block_id);
        self.blocks.borrow_mut().sort_by_key(|block| block.height);
        bdk_electrum_streaming::Update {
            tx_update,
            chain_update: Some(
                CheckPoint::from_block_ids(self.blocks.borrow().iter().copied()).unwrap(),
            ),
            last_active_indices: last_active
                .into_iter()
                .map(|(account_keychain, index)| ((self.master_appkey, account_keychain), index))
                .collect(),
        }
    }

    pub(crate) fn deliver(
        &self,
        wallet: &mut CoordSuperWallet,
        txs: Vec<bitcoin::Transaction>,
        height: u32,
        last_active: Option<(BitcoinAccountKeychain, u32)>,
    ) {
        wallet
            .apply_update(self.update(txs, height, last_active))
            .unwrap();
    }

    pub(crate) fn reload_scan_state(&self) -> super::recovery_scan::RecoveryScanState {
        let mut conn = self.db.lock().unwrap();
        let persisted: Persisted<super::recovery_scan::RecoveryScanState> =
            Persisted::new(&mut *conn, ()).unwrap();
        (*persisted).clone()
    }
}

pub(crate) fn foreign_spk() -> ScriptBuf {
    let key = MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
    super::peek_spk(key, BitcoinBip32Path::external(0))
}

pub(crate) fn tx_paying(
    inputs: Vec<OutPoint>,
    outputs: Vec<(ScriptBuf, u64)>,
) -> bitcoin::Transaction {
    bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: if inputs.is_empty() {
            vec![bitcoin::TxIn::default()]
        } else {
            inputs
                .into_iter()
                .map(|previous_output| bitcoin::TxIn {
                    previous_output,
                    ..Default::default()
                })
                .collect()
        },
        output: outputs
            .into_iter()
            .map(|(script_pubkey, value)| TxOut {
                value: Amount::from_sat(value),
                script_pubkey,
            })
            .collect(),
    }
}

/// Receive on external 0, then spend it all to outputs of which `change` may pay us back at an
/// internal index far past the reveal frontier.
pub(crate) fn fund_and_burn(
    fixture: &Fixture,
    wallet: &mut CoordSuperWallet,
    change: Option<(u32, u64)>,
) -> (Txid, Txid) {
    let receive = tx_paying(vec![], vec![(fixture.spk(EXTERNAL(), 0), 100_000)]);
    let receive_txid = receive.compute_txid();
    fixture.deliver(wallet, vec![receive], 100, Some((EXTERNAL(), 0)));

    let change_output = match change {
        Some((index, value)) => (fixture.spk(INTERNAL(), index), value),
        None => (foreign_spk(), 30_000),
    };
    let burn = tx_paying(
        vec![OutPoint {
            txid: receive_txid,
            vout: 0,
        }],
        vec![(foreign_spk(), 60_000), change_output],
    );
    let burn_txid = burn.compute_txid();
    fixture.deliver(wallet, vec![burn], 101, None);
    (receive_txid, burn_txid)
}

pub(crate) fn unspent_sum(wallet: &CoordSuperWallet, master_appkey: MasterAppkey) -> u64 {
    wallet
        .tx_graph
        .graph()
        .filter_chain_unspents(
            wallet.chain.as_ref(),
            wallet.chain.tip().block_id(),
            CanonicalizationParams::default(),
            wallet
                .tx_graph
                .index
                .keychain_outpoints_in_range(CoordSuperWallet::key_index_range(master_appkey)),
        )
        .map(|(_, utxo)| utxo.txout.value.to_sat())
        .sum()
}

pub(crate) fn last_revealed_rows(db: &Arc<Mutex<rusqlite::Connection>>) -> Vec<(String, u32)> {
    let conn = db.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT descriptor_id, last_revealed FROM bdk_descriptor_last_revealed \
             ORDER BY descriptor_id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows
}
