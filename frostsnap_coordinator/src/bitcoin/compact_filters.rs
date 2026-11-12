//! Compact block filter matching for the BIP157 backend.

use super::wallet::{KeychainId, WalletIndexer};
use bdk_chain::{
    bitcoin::{Block, OutPoint, ScriptBuf, Transaction},
    miniscript::{Descriptor, DescriptorPublicKey},
    spk_client::FullScanResponse,
    BlockId, ConfirmationBlockTime, SpkIterator, TxUpdate,
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
    /// Our coins, with the keychain index that received each.
    owned: HashMap<OutPoint, (KeychainId, u32)>,
    /// Scripts kept watched past the highest index a payment has used.
    lookahead: u32,
    /// Each keychain's descriptor, so its window can move past a payment as soon as one is found
    /// rather than once the wallet has applied it.
    windows: HashMap<KeychainId, Window>,
}

#[derive(Debug, Clone)]
struct Window {
    descriptor: Descriptor<DescriptorPublicKey>,
    /// The first index not yet derived.
    end: u32,
}

impl WatchSet {
    pub fn new(
        spks: impl IntoIterator<Item = (ScriptBuf, (KeychainId, u32))>,
        owned: impl IntoIterator<Item = (OutPoint, (KeychainId, u32))>,
    ) -> Self {
        Self {
            spks: spks.into_iter().collect(),
            owned: owned.into_iter().collect(),
            ..Default::default()
        }
    }

    /// Derive every keychain out to its frontier plus `lookahead`, and hold every coin the
    /// indexer has attributed to one of them.
    pub fn from_indexer(indexer: &WalletIndexer, lookahead: u32) -> Self {
        let owned = indexer
            .outpoints()
            .iter()
            .map(|&(keychain_index, outpoint)| (outpoint, keychain_index))
            .collect();
        let mut watch = Self {
            owned,
            lookahead,
            ..Default::default()
        };
        for (keychain, descriptor) in indexer.keychains() {
            let window = Window {
                descriptor: descriptor.clone(),
                end: 0,
            };
            watch.windows.insert(keychain, window);
            match indexer.last_revealed_index(keychain) {
                Some(frontier) => watch.watch_past(keychain, frontier),
                None => watch.derive_until(keychain, lookahead),
            }
        }
        watch
    }

    /// Watch `lookahead` scripts past `index` on `keychain`.
    fn watch_past(&mut self, keychain: KeychainId, index: u32) {
        let end = index.saturating_add(self.lookahead).saturating_add(1);
        self.derive_until(keychain, end);
    }

    /// Derive `keychain`'s scripts up to, but not including, `end`.
    fn derive_until(&mut self, keychain: KeychainId, end: u32) {
        let Some(window) = self.windows.get_mut(&keychain) else {
            return;
        };
        if end <= window.end {
            return;
        }
        for (index, spk) in SpkIterator::new_with_range(&window.descriptor, window.end..end) {
            self.spks.insert(spk, (keychain, index));
        }
        window.end = end;
    }

    pub fn is_empty(&self) -> bool {
        self.spks.is_empty()
    }

    pub fn scripts(&self) -> impl Iterator<Item = &ScriptBuf> + '_ {
        self.spks.keys()
    }

    pub fn owned(&self) -> impl Iterator<Item = &OutPoint> + '_ {
        self.owned.keys()
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
        let mut ours = HashSet::new();
        loop {
            let watched = self.spks.len();
            for tx in &block.txdata {
                if !self.record_if_ours(tx, &mut last_active_indices) {
                    continue;
                }
                let txid = tx.compute_txid();
                if ours.insert(txid) {
                    tx_update.txs.push(Arc::new(tx.clone()));
                    tx_update.anchors.insert((anchor, txid));
                }
            }
            // A payment found in this pass may have moved the window onto scripts that a
            // transaction already passed over pays, so go again until it stops moving.
            if self.spks.len() == watched {
                break;
            }
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

    /// Our transaction from mempool gossip, recorded as seen at `seen_at` (unix seconds).
    pub fn scan_unconfirmed(
        &mut self,
        tx: &Transaction,
        seen_at: u64,
    ) -> Option<FullScanResponse<KeychainId, ConfirmationBlockTime>> {
        let mut last_active_indices = BTreeMap::<KeychainId, u32>::new();
        if !self.record_if_ours(tx, &mut last_active_indices) {
            return None;
        }
        let mut tx_update = TxUpdate::<ConfirmationBlockTime>::default();
        tx_update.txs.push(Arc::new(tx.clone()));
        tx_update.seen_ats.insert((tx.compute_txid(), seen_at));
        Some(FullScanResponse {
            tx_update,
            last_active_indices,
            chain_update: None,
        })
    }

    /// If `tx` is ours, record the coins it pays us and raise each keychain's last active index to
    /// the highest it uses, moving the watched window past it. A spend that pays us nothing still
    /// records the spent coins' keychains in `last_active_indices`, so those wallets refresh.
    /// Recording the same transaction again changes nothing.
    fn record_if_ours(
        &mut self,
        tx: &Transaction,
        last_active_indices: &mut BTreeMap<KeychainId, u32>,
    ) -> bool {
        if !self.is_relevant(tx) {
            return false;
        }
        let mut note_last_active = |(keychain, index): (KeychainId, u32)| {
            last_active_indices
                .entry(keychain)
                .and_modify(|last| *last = (*last).max(index))
                .or_insert(index);
        };
        for txin in &tx.input {
            if let Some(&spent) = self.owned.get(&txin.previous_output) {
                note_last_active(spent);
            }
        }
        let txid = tx.compute_txid();
        for (vout, txout) in tx.output.iter().enumerate() {
            if let Some(&received) = self.spks.get(&txout.script_pubkey) {
                let vout = vout as u32;
                self.owned.insert(OutPoint { txid, vout }, received);
                note_last_active(received);
                let (keychain, index) = received;
                self.watch_past(keychain, index);
            }
        }
        true
    }

    /// Ours if it pays a script we know or spends a coin we hold.
    fn is_relevant(&self, tx: &Transaction) -> bool {
        tx.output
            .iter()
            .any(|txout| self.spks.contains_key(&txout.script_pubkey))
            || tx
                .input
                .iter()
                .any(|txin| self.owned.contains_key(&txin.previous_output))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bdk_chain::bitcoin::{
        absolute::LockTime,
        block::{Header, Version},
        hashes::Hash,
        transaction, Amount, BlockHash, CompactTarget, OutPoint, Sequence, Transaction, TxIn,
        TxMerkleNode, TxOut, Witness,
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

    fn outpoint(tx: &Transaction, vout: u32) -> OutPoint {
        OutPoint {
            txid: tx.compute_txid(),
            vout,
        }
    }

    #[test]
    fn scan_block_anchors_payment_at_block_height_and_time() {
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
    fn scan_unconfirmed_records_seen_at_without_anchor() {
        let k = keychain();
        let ours = spk(1);
        let payment = tx(&[OutPoint::null()], &[(ours.clone(), 10_000)]);
        let mut watch = WatchSet::new([(ours, (k, 3))], []);
        let update = watch
            .scan_unconfirmed(&payment, 1_800_000_000)
            .expect("ours");
        assert_eq!(
            update.tx_update.txs[0].compute_txid(),
            payment.compute_txid()
        );
        assert!(update.tx_update.anchors.is_empty(), "not in a block yet");
        assert!(update
            .tx_update
            .seen_ats
            .contains(&(payment.compute_txid(), 1_800_000_000)));
        assert_eq!(update.last_active_indices.get(&k), Some(&3));
        let coin = OutPoint {
            txid: payment.compute_txid(),
            vout: 0,
        };
        assert!(
            watch.owned().any(|owned| *owned == coin),
            "the coin it pays is ours to follow"
        );
    }

    #[test]
    fn scan_block_ignores_block_without_our_txs() {
        let k = keychain();
        let block = block(vec![tx(&[OutPoint::null()], &[(spk(2), 10_000)])]);
        let mut watch = WatchSet::new([(spk(1), (k, 0))], []);
        assert!(watch.scan_block(&block, 1).is_none());
    }

    #[test]
    fn scan_block_detects_spend_of_owned_coin_without_change() {
        let k = keychain();
        let coin = OutPoint {
            txid: bdk_chain::bitcoin::Txid::all_zeros(),
            vout: 0,
        };
        let spend = tx(&[coin], &[(spk(9), 9_000)]);
        let block = block(vec![spend.clone()]);
        let mut watch = WatchSet::new([(spk(1), (k, 0))], [(coin, (k, 4))]);
        let update = watch.scan_block(&block, 5).expect("spends our coin");
        assert_eq!(update.tx_update.txs.len(), 1);
        assert_eq!(update.tx_update.txs[0].compute_txid(), spend.compute_txid());
        assert_eq!(
            update.last_active_indices.get(&k),
            Some(&4),
            "the wallet whose coin was spent is told, though nothing came back to it"
        );
    }

    #[test]
    fn scan_block_tracks_coin_created_and_spent_in_same_block() {
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

        /// A fresh wallet's external keychain, and the highest index its watch set starts with.
        fn external_window(
            wallet: &CoordSuperWallet,
            master_appkey: MasterAppkey,
        ) -> (KeychainId, u32) {
            let external = (master_appkey, BitcoinAccountKeychain::external());
            let last_watched = match wallet.tx_graph.index.last_revealed_index(external) {
                Some(frontier) => frontier + MATCH_LOOKAHEAD,
                None => MATCH_LOOKAHEAD - 1,
            };
            (external, last_watched)
        }

        fn script(wallet: &CoordSuperWallet, keychain: KeychainId, index: u32) -> ScriptBuf {
            wallet
                .tx_graph
                .index
                .spk_at_index(keychain, index)
                .expect("within the wallet's lookahead")
        }

        /// A payment beyond the window comes first in the block, and only the payment after it
        /// moves the window far enough to see it.
        #[test]
        fn scan_block_finds_payment_the_window_reaches_later_in_the_block() {
            let (wallet, _handler, master_appkey) = wallet();
            let (external, last) = external_window(&wallet, master_appkey);
            let mut watch = WatchSet::from_indexer(&wallet.tx_graph.index, MATCH_LOOKAHEAD);
            let far = tx(
                &[OutPoint::null()],
                &[(script(&wallet, external, last + 40), 1_000)],
            );
            let near = tx(
                &[OutPoint::null()],
                &[(script(&wallet, external, last), 2_000)],
            );
            let update = watch
                .scan_block(&block(vec![far.clone(), near.clone()]), 1)
                .expect("both pay us");
            let txids = update
                .tx_update
                .txs
                .iter()
                .map(|tx| tx.compute_txid())
                .collect::<HashSet<_>>();
            assert_eq!(
                txids,
                HashSet::from([far.compute_txid(), near.compute_txid()])
            );
            assert_eq!(
                update.last_active_indices.get(&external),
                Some(&(last + 40))
            );
            assert!(watch.owned().any(|coin| *coin == outpoint(&far, 0)));
        }

        /// One transaction pays an index beyond the window before one inside it; both coins are
        /// ours, so a later spend of either is still recognised.
        #[test]
        fn scan_block_records_every_output_once_the_window_moves() {
            let (wallet, _handler, master_appkey) = wallet();
            let (external, last) = external_window(&wallet, master_appkey);
            let mut watch = WatchSet::from_indexer(&wallet.tx_graph.index, MATCH_LOOKAHEAD);
            let payment = tx(
                &[OutPoint::null()],
                &[
                    (script(&wallet, external, last + 40), 1_000),
                    (script(&wallet, external, last), 2_000),
                ],
            );
            let update = watch
                .scan_block(&block(vec![payment.clone()]), 1)
                .expect("pays us");
            assert_eq!(update.tx_update.txs.len(), 1);
            assert_eq!(
                update.last_active_indices.get(&external),
                Some(&(last + 40))
            );
            let spend = tx(&[outpoint(&payment, 0)], &[(spk(9), 900)]);
            assert!(
                watch.scan_block(&block(vec![spend]), 2).is_some(),
                "the coin beyond the old window is ours to follow"
            );
        }

        /// Each payment lands 40 past the one before, always beyond where the window started.
        #[test]
        fn scan_block_follows_payments_across_blocks_without_a_refresh() {
            let (wallet, _handler, master_appkey) = wallet();
            let (external, last) = external_window(&wallet, master_appkey);
            let mut watch = WatchSet::from_indexer(&wallet.tx_graph.index, MATCH_LOOKAHEAD);
            for (height, index) in (1..=5).zip((0..5).map(|step| last + step * 40)) {
                let payment = tx(
                    &[OutPoint::null()],
                    &[(script(&wallet, external, index), 1_000)],
                );
                let update = watch
                    .scan_block(&block(vec![payment]), height)
                    .unwrap_or_else(|| panic!("payment to index {index} was missed"));
                assert_eq!(update.last_active_indices.get(&external), Some(&index));
            }
        }
    }
}
