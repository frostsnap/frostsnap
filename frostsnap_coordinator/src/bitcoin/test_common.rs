//! Test fixtures shared by the `bitcoin` module's tests.
use super::send::*;
use crate::bitcoin::chain_sync::{ChainClient, ConnectionHandler, ElectrumConfig};
use crate::bitcoin::wallet::CoordSuperWallet;
use crate::persist::Persisted;
use crate::settings::ElectrumEnabled;
use anyhow::Result;
use bdk_chain::{
    bitcoin::{self, hashes::Hash, Amount, BlockHash, OutPoint, TxIn, TxOut},
    BlockId, CheckPoint, ConfirmationBlockTime, TxUpdate,
};
use frostsnap_core::schnorr_fun::fun::Point;
use frostsnap_core::tweak::{BitcoinAccountKeychain, BitcoinBip32Path, NormalIndex};
use frostsnap_core::MasterAppkey;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

pub(super) const NETWORK: bitcoin::Network = bitcoin::Network::Bitcoin;

/// The handler owns the receiving ends of the client's channels, so it must outlive every
/// `ChainClient` call or `monitor_keychain`'s send panics.
pub(super) fn chain_client(
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> (ChainClient, ConnectionHandler) {
    let trusted = {
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
        trusted,
        db.clone(),
    )
}

pub(super) struct Fixture {
    pub(super) wallet: CoordSuperWallet,
    pub(super) handler: ConnectionHandler,
    pub(super) master_appkey: MasterAppkey,
    pub(super) recipient: bitcoin::Address,
    pub(super) blocks: Vec<BlockId>,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
        let (client, handler) = chain_client(&db);
        let master_appkey =
            MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
        let mut wallet = CoordSuperWallet::load_or_init(db, NETWORK, client).unwrap();
        wallet.list_addresses(master_appkey);
        let recipient = bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
            .unwrap()
            .require_network(NETWORK)
            .unwrap();
        Self {
            wallet,
            handler,
            master_appkey,
            recipient,
            blocks: vec![BlockId {
                height: 0,
                hash: bitcoin::constants::genesis_block(NETWORK).block_hash(),
            }],
        }
    }

    /// Deliver a confirmed external payment the way a sync would.
    pub(super) fn fund(&mut self, index: u32, value: u64, height: u32) -> OutPoint {
        self.fund_keychain(BitcoinAccountKeychain::external(), index, value, height)
    }

    /// Deliver a confirmed payment to either keychain. Change lands on the internal one,
    /// which is where the burned indices are.
    pub(super) fn fund_keychain(
        &mut self,
        account_keychain: BitcoinAccountKeychain,
        index: u32,
        value: u64,
        height: u32,
    ) -> OutPoint {
        let spk = crate::bitcoin::peek_spk(
            self.master_appkey,
            BitcoinBip32Path {
                account_keychain,
                index: NormalIndex::new(index).expect("fixture index is a literal below 2^31"),
            },
        );
        let tx = bitcoin::Transaction {
            version: bitcoin::transaction::Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn::default()],
            output: vec![TxOut {
                value: Amount::from_sat(value),
                script_pubkey: spk,
            }],
        };
        let block = BlockId {
            height,
            hash: BlockHash::from_byte_array([height as u8; 32]),
        };
        self.blocks.push(block);
        let mut tx_update = TxUpdate::default();
        tx_update.txs = vec![Arc::new(tx.clone())];
        tx_update.anchors = [(
            ConfirmationBlockTime {
                block_id: block,
                confirmation_time: 1_700_000_000,
            },
            tx.compute_txid(),
        )]
        .into();
        self.wallet
            .apply_update(bdk_electrum_streaming::Update {
                tx_update,
                last_active_indices: [((self.master_appkey, account_keychain), index)].into(),
                chain_update: Some(
                    CheckPoint::from_block_ids(self.blocks.iter().copied()).unwrap(),
                ),
            })
            .unwrap();
        OutPoint {
            txid: tx.compute_txid(),
            vout: 0,
        }
    }

    /// Deliver a spend of one of our coins that pays change to `change_index`, with the server
    /// naming nothing.
    ///
    /// This is the shape the whole recovery rests on and the one `fund_keychain` cannot make: a
    /// server only reports activity on scripts it subscribes to, so change sent past that window
    /// arrives as an unattributed output of a transaction we hold only because we own a spent
    /// prevout. Nothing in the update points at it.
    pub(super) fn spend_paying_unannounced_change(
        &mut self,
        spend: OutPoint,
        change_index: u32,
        change_value: u64,
        height: u32,
    ) {
        let change_spk = crate::bitcoin::peek_spk(
            self.master_appkey,
            BitcoinBip32Path {
                account_keychain: BitcoinAccountKeychain::internal(),
                index: NormalIndex::new(change_index).expect("fixture index below 2^31"),
            },
        );
        let tx = bitcoin::Transaction {
            version: bitcoin::transaction::Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: spend,
                ..Default::default()
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(10_000),
                    script_pubkey: self.recipient.script_pubkey(),
                },
                TxOut {
                    value: Amount::from_sat(change_value),
                    script_pubkey: change_spk,
                },
            ],
        };
        let block = BlockId {
            height,
            hash: BlockHash::from_byte_array([height as u8; 32]),
        };
        self.blocks.push(block);
        let mut tx_update = TxUpdate::default();
        tx_update.txs = vec![Arc::new(tx.clone())];
        tx_update.anchors = [(
            ConfirmationBlockTime {
                block_id: block,
                confirmation_time: 1_700_000_000,
            },
            tx.compute_txid(),
        )]
        .into();
        self.wallet
            .apply_update(bdk_electrum_streaming::Update {
                tx_update,
                last_active_indices: Default::default(),
                chain_update: Some(
                    CheckPoint::from_block_ids(self.blocks.iter().copied()).unwrap(),
                ),
            })
            .unwrap();
    }

    pub(super) fn last_revealed_internal(&self) -> Option<u32> {
        self.wallet
            .tx_graph
            .index
            .last_revealed_index((self.master_appkey, BitcoinAccountKeychain::internal()))
    }

    pub(super) fn plan_all(&mut self, feerate: f32) -> Result<SendPlan> {
        let outpoints = self
            .wallet
            .all_unspent_outpoints(self.master_appkey, feerate);
        self.wallet
            .plan_consolidate(self.master_appkey, outpoints, feerate)
    }

    /// The input set the nudge's remedy consolidates, composed the way its call site does.
    pub(super) fn plan_stranded(&mut self, feerate: f32) -> Result<SendPlan> {
        let outpoints = self
            .wallet
            .gap_stranded_outpoints(self.master_appkey, feerate);
        self.wallet
            .plan_consolidate(self.master_appkey, outpoints, feerate)
    }

    pub(super) fn plan(&mut self, sats: u64) -> SendPlan {
        self.wallet
            .plan_send(
                self.master_appkey,
                [(self.recipient.clone(), Some(sats))],
                1.0,
            )
            .unwrap()
    }
}
