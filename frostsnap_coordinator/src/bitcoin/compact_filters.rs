//! Compact block filter matching for the BIP157 backend.

use super::wallet::{KeychainId, WalletIndexer};
use anyhow::{Context, Result};
use bdk_chain::{
    bitcoin::{bip158::BlockFilter, Block, BlockHash, OutPoint, ScriptBuf, Transaction},
    spk_client::FullScanResponse,
    BlockId, CheckPoint, ConfirmationBlockTime, TxUpdate,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

/// Scripts derived past each keychain's frontier for matching against filters.
pub const MATCH_LOOKAHEAD: u32 = 50;

/// Snapshot of the wallet's scripts and owned coins for lock-free matching.
#[derive(Debug, Clone, Default)]
pub struct WatchSet {
    spks: HashMap<ScriptBuf, (KeychainId, u32)>,
    owned: HashSet<OutPoint>,
}

impl WatchSet {
    pub fn new(
        spks: impl IntoIterator<Item = (ScriptBuf, (KeychainId, u32))>,
        owned: impl IntoIterator<Item = OutPoint>,
    ) -> Self {
        Self {
            spks: spks.into_iter().collect(),
            owned: owned.into_iter().collect(),
        }
    }

    /// Derive every keychain out to its frontier plus `lookahead`.
    pub fn from_indexer(
        indexer: &WalletIndexer,
        lookahead: u32,
        owned: impl IntoIterator<Item = OutPoint>,
    ) -> Self {
        let keychains = indexer
            .keychains()
            .map(|(keychain, _)| keychain)
            .collect::<Vec<_>>();
        let mut spks = HashMap::new();
        for keychain in keychains {
            let frontier = indexer.last_revealed_index(keychain);
            let last = match frontier {
                Some(frontier) => frontier.saturating_add(lookahead),
                None => lookahead.saturating_sub(1),
            };
            for index in 0..=last {
                if let Some(spk) = indexer.spk_at_index(keychain, index) {
                    spks.insert(spk, (keychain, index));
                }
            }
        }
        Self {
            spks,
            owned: owned.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.spks.is_empty()
    }

    pub fn scripts(&self) -> impl Iterator<Item = &ScriptBuf> + '_ {
        self.spks.keys()
    }

    /// Whether `filter` says this block might concern us.
    pub fn matches(&self, block_hash: &BlockHash, filter: &BlockFilter) -> Result<bool> {
        if self.spks.is_empty() {
            return Ok(false);
        }
        filter
            .match_any(block_hash, self.spks.keys().map(|spk| spk.as_bytes()))
            .context("reading compact block filter")
    }

    /// Extract our transactions from a fetched block; `chain_update` is left for the caller.
    pub fn scan_block(
        &mut self,
        block: &Block,
        height: u32,
    ) -> Option<FullScanResponse<KeychainId, ConfirmationBlockTime>> {
        let block_id = BlockId {
            height,
            hash: block.block_hash(),
        };
        let anchor = ConfirmationBlockTime {
            block_id,
            confirmation_time: block.header.time.into(),
        };
        let mut tx_update = TxUpdate::<ConfirmationBlockTime>::default();
        let mut last_active_indices = BTreeMap::<KeychainId, u32>::new();
        for tx in &block.txdata {
            if !self.is_relevant(tx) {
                continue;
            }
            let txid = tx.compute_txid();
            for (vout, txout) in tx.output.iter().enumerate() {
                if let Some(&(keychain, index)) = self.spks.get(&txout.script_pubkey) {
                    self.owned.insert(OutPoint {
                        txid,
                        vout: vout as u32,
                    });
                    last_active_indices
                        .entry(keychain)
                        .and_modify(|last| *last = (*last).max(index))
                        .or_insert(index);
                }
            }
            tx_update.txs.push(Arc::new(tx.clone()));
            tx_update.anchors.insert((anchor, txid));
        }
        if tx_update.txs.is_empty() {
            return None;
        }
        Some(FullScanResponse {
            tx_update,
            last_active_indices,
            chain_update: None,
        })
    }

    /// Ours if it pays a script we know or spends a coin we hold.
    fn is_relevant(&self, tx: &Transaction) -> bool {
        tx.output
            .iter()
            .any(|txout| self.spks.contains_key(&txout.script_pubkey))
            || tx
                .input
                .iter()
                .any(|txin| self.owned.contains(&txin.previous_output))
    }
}

/// Insert `block_id` into the chain, extending the tip or replacing it on reorg.
pub fn connect(tip: CheckPoint, block_id: BlockId) -> CheckPoint {
    tip.insert(block_id)
}

#[cfg(test)]
mod test {
    use super::*;
    use bdk_chain::bitcoin::{
        absolute::LockTime,
        block::{Header, Version},
        hashes::Hash,
        transaction, Amount, CompactTarget, OutPoint, Sequence, Transaction, TxIn, TxMerkleNode,
        TxOut, Witness,
    };
    use frostsnap_core::{tweak::BitcoinAccountKeychain, MasterAppkey};

    const TIME: u32 = 1_700_000_000;

    fn keychain() -> KeychainId {
        let appkey = MasterAppkey::derive_from_rootkey(
            frostsnap_core::schnorr_fun::fun::Point::random(&mut rand::thread_rng()),
        );
        (appkey, BitcoinAccountKeychain::external())
    }

    fn spk(byte: u8) -> ScriptBuf {
        let mut script = vec![0x51, 0x20];
        script.extend([byte; 32]);
        ScriptBuf::from_bytes(script)
    }

    fn tx(inputs: &[OutPoint], outputs: &[(ScriptBuf, u64)]) -> Transaction {
        Transaction {
            version: transaction::Version::TWO,
            lock_time: LockTime::ZERO,
            input: inputs
                .iter()
                .map(|previous_output| TxIn {
                    previous_output: *previous_output,
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    witness: Witness::new(),
                })
                .collect(),
            output: outputs
                .iter()
                .map(|(script_pubkey, value)| TxOut {
                    script_pubkey: script_pubkey.clone(),
                    value: Amount::from_sat(*value),
                })
                .collect(),
        }
    }

    fn block(txdata: Vec<Transaction>) -> Block {
        Block {
            header: Header {
                version: Version::TWO,
                prev_blockhash: BlockHash::all_zeros(),
                merkle_root: TxMerkleNode::all_zeros(),
                time: TIME,
                bits: CompactTarget::from_consensus(0x1d00_ffff),
                nonce: 0,
            },
            txdata,
        }
    }

    fn filter_for(block: &Block, prevouts: &[(OutPoint, ScriptBuf)]) -> BlockFilter {
        let map: HashMap<OutPoint, ScriptBuf> = prevouts.iter().cloned().collect();
        BlockFilter::new_script_filter(block, |outpoint| {
            map.get(outpoint)
                .cloned()
                .ok_or(bdk_chain::bitcoin::bip158::Error::UtxoMissing(*outpoint))
        })
        .expect("every spent prevout is supplied")
    }

    fn outpoint(tx: &Transaction, vout: u32) -> OutPoint {
        OutPoint {
            txid: tx.compute_txid(),
            vout,
        }
    }

    #[test]
    fn a_filter_naming_one_of_our_scripts_matches() {
        let k = keychain();
        let ours = spk(1);
        let block = block(vec![tx(&[OutPoint::null()], &[(ours.clone(), 10_000)])]);
        let filter = filter_for(&block, &[]);
        let watch = WatchSet::new([(ours, (k, 0))], []);
        assert!(watch.matches(&block.block_hash(), &filter).unwrap());
    }

    #[test]
    fn a_filter_naming_nothing_of_ours_does_not_match() {
        let k = keychain();
        let block = block(vec![tx(&[OutPoint::null()], &[(spk(2), 10_000)])]);
        let filter = filter_for(&block, &[]);
        let watch = WatchSet::new([(spk(1), (k, 0))], []);
        assert!(!watch.matches(&block.block_hash(), &filter).unwrap());
    }

    #[test]
    fn an_empty_watch_set_matches_nothing() {
        let block = block(vec![tx(&[OutPoint::null()], &[(spk(1), 10_000)])]);
        let filter = filter_for(&block, &[]);
        let watch = WatchSet::default();
        assert!(!watch.matches(&block.block_hash(), &filter).unwrap());
    }

    #[test]
    fn a_payment_to_us_is_captured_with_its_height_and_time() {
        let k = keychain();
        let ours = spk(1);
        let payment = tx(&[OutPoint::null()], &[(ours.clone(), 10_000)]);
        let block = block(vec![payment.clone()]);
        let mut watch = WatchSet::new([(ours, (k, 7))], []);
        let update = watch.scan_block(&block, 812_345).expect("ours");
        assert_eq!(update.tx_update.txs.len(), 1);
        assert_eq!(
            update.tx_update.txs[0].compute_txid(),
            payment.compute_txid()
        );
        assert_eq!(update.last_active_indices.get(&k), Some(&7));
        let (anchor, txid) = update.tx_update.anchors.iter().next().unwrap();
        assert_eq!(*txid, payment.compute_txid());
        assert_eq!(anchor.block_id.height, 812_345);
        assert_eq!(anchor.block_id.hash, block.block_hash());
        assert_eq!(anchor.confirmation_time, TIME as u64);
    }

    #[test]
    fn a_block_holding_nothing_of_ours_yields_no_update() {
        let k = keychain();
        let block = block(vec![tx(&[OutPoint::null()], &[(spk(2), 10_000)])]);
        let mut watch = WatchSet::new([(spk(1), (k, 0))], []);
        assert!(watch.scan_block(&block, 1).is_none());
    }

    #[test]
    fn spending_our_coin_is_relevant_even_when_every_output_is_a_stranger() {
        let k = keychain();
        let coin = OutPoint {
            txid: bdk_chain::bitcoin::Txid::all_zeros(),
            vout: 0,
        };
        let spend = tx(&[coin], &[(spk(9), 9_000)]);
        let block = block(vec![spend.clone()]);
        let mut watch = WatchSet::new([(spk(1), (k, 0))], [coin]);
        let update = watch.scan_block(&block, 5).expect("spends our coin");
        assert_eq!(update.tx_update.txs.len(), 1);
        assert_eq!(update.tx_update.txs[0].compute_txid(), spend.compute_txid());
        assert!(update.last_active_indices.is_empty());
    }

    #[test]
    fn a_coin_created_and_spent_in_one_block_is_followed() {
        let k = keychain();
        let ours = spk(1);
        let receive = tx(&[OutPoint::null()], &[(ours.clone(), 10_000)]);
        let spend = tx(&[outpoint(&receive, 0)], &[(spk(9), 9_000)]);
        let block = block(vec![receive.clone(), spend.clone()]);
        let mut watch = WatchSet::new([(ours, (k, 0))], []);
        let update = watch.scan_block(&block, 11).expect("both are ours");
        let txids = update
            .tx_update
            .txs
            .iter()
            .map(|tx| tx.compute_txid())
            .collect::<Vec<_>>();
        assert_eq!(txids, vec![receive.compute_txid(), spend.compute_txid()]);
    }

    #[test]
    fn the_highest_index_seen_is_the_one_reported() {
        let k = keychain();
        let low = spk(1);
        let high = spk(2);
        let block = block(vec![tx(
            &[OutPoint::null()],
            &[(high.clone(), 1), (low.clone(), 2)],
        )]);
        let mut watch = WatchSet::new([(low, (k, 3)), (high, (k, 9))], []);
        let update = watch.scan_block(&block, 1).expect("ours");
        assert_eq!(update.last_active_indices.get(&k), Some(&9));
    }

    #[test]
    fn connect_extends_the_tip() {
        let genesis = BlockId {
            height: 0,
            hash: BlockHash::all_zeros(),
        };
        let next = BlockId {
            height: 1,
            hash: BlockHash::from_byte_array([1; 32]),
        };
        let cp = connect(CheckPoint::new(genesis), next);
        assert_eq!(cp.height(), 1);
        assert_eq!(cp.hash(), next.hash);
    }

    #[test]
    fn connect_purges_a_reorged_block_and_everything_above_it() {
        let genesis = BlockId {
            height: 0,
            hash: BlockHash::all_zeros(),
        };
        let one_a = BlockId {
            height: 1,
            hash: BlockHash::from_byte_array([1; 32]),
        };
        let two_a = BlockId {
            height: 2,
            hash: BlockHash::from_byte_array([2; 32]),
        };
        let one_b = BlockId {
            height: 1,
            hash: BlockHash::from_byte_array([0xb1; 32]),
        };
        let cp = connect(connect(CheckPoint::new(genesis), one_a), two_a);
        assert_eq!(cp.height(), 2);
        let reorged = connect(cp, one_b);
        assert_eq!(reorged.height(), 1);
        assert_eq!(reorged.hash(), one_b.hash);
        let heights = reorged.iter().map(|cp| cp.height()).collect::<Vec<_>>();
        assert_eq!(heights, vec![1, 0]);
    }

    mod round_trip {
        use super::*;
        use crate::bitcoin::chain_sync::{ChainClient, ConnectionHandler, ElectrumConfig};
        use crate::bitcoin::wallet::CoordSuperWallet;
        use crate::persist::Persisted;
        use crate::settings::ElectrumEnabled;
        use bdk_chain::bitcoin;
        use frostsnap_core::schnorr_fun::fun::Point;
        use std::sync::Mutex;

        const NETWORK: bitcoin::Network = bitcoin::Network::Bitcoin;

        fn wallet() -> (CoordSuperWallet, ConnectionHandler, MasterAppkey) {
            let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
            let trusted = {
                let mut conn = db.lock().unwrap();
                Persisted::new(&mut *conn, NETWORK).unwrap()
            };
            let (client, handler) = ChainClient::new(
                bitcoin::constants::genesis_block(NETWORK).block_hash(),
                ElectrumConfig {
                    enabled: ElectrumEnabled::None,
                    primary: String::new(),
                    backup: String::new(),
                },
                trusted,
                db.clone(),
            );
            let master_appkey =
                MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
            let mut wallet = CoordSuperWallet::load_or_init(db, NETWORK, client).unwrap();
            wallet.list_addresses(master_appkey);
            (wallet, handler, master_appkey)
        }

        fn block_id(height: u32, hash: [u8; 32]) -> BlockId {
            BlockId {
                height,
                hash: BlockHash::from_byte_array(hash),
            }
        }

        #[test]
        fn a_confirmed_payment_survives_until_its_block_is_reorged_away() {
            let (mut wallet, _handler, master_appkey) = wallet();
            let external = (master_appkey, BitcoinAccountKeychain::external());
            let mut watch = WatchSet::from_indexer(&wallet.tx_graph.index, MATCH_LOOKAHEAD, []);
            let ours = wallet
                .tx_graph
                .index
                .spk_at_index(external, 0)
                .expect("first external script is derivable");
            let payment = tx(&[OutPoint::null()], &[(ours, 10_000)]);
            let block_a = block(vec![payment.clone()]);
            let mut update = watch.scan_block(&block_a, 1).expect("pays us");
            let id_a = BlockId {
                height: 1,
                hash: block_a.block_hash(),
            };
            update.chain_update = Some(connect(wallet.chain_tip(), id_a));
            assert!(wallet.apply_update(update).unwrap(), "the wallet changed");
            assert_eq!(wallet.chain_tip().hash(), id_a.hash);
            let seen = wallet.list_transactions(master_appkey);
            assert_eq!(seen.len(), 1, "the payment is visible");
            let block_b_id = block_id(1, [0xb1; 32]);
            let reorg = FullScanResponse {
                tx_update: TxUpdate::default(),
                last_active_indices: BTreeMap::default(),
                chain_update: Some(connect(wallet.chain_tip(), block_b_id)),
            };
            assert!(wallet.apply_update(reorg).unwrap(), "the chain changed");
            assert_eq!(
                wallet.chain_tip().hash(),
                block_b_id.hash,
                "the wallet followed the new chain"
            );
            assert!(
                wallet.list_transactions(master_appkey).is_empty(),
                "a payment anchored only to an abandoned block is no longer the wallet's"
            );
        }
    }
}
