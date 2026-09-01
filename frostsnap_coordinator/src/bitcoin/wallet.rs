use super::{chain_sync::ChainClient, multi_x_descriptor_for_account};
use crate::persist::Persisted;
use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self, Amount, BlockHash, OutPoint, ScriptBuf, TxOut, Txid},
    indexed_tx_graph::{self},
    indexer::keychain_txout::{self, KeychainTxOutIndex},
    local_chain,
    miniscript::{Descriptor, DescriptorPublicKey},
    CanonicalizationParams, ChainPosition, CheckPoint, ConfirmationBlockTime, Merge,
};
use frostsnap_core::{
    tweak::{BitcoinAccount, BitcoinAccountKeychain, BitcoinBip32Path, NormalIndex},
    MasterAppkey,
};
use std::{
    collections::HashMap,
    ops::RangeBounds,
    str::FromStr,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};

/// Scripts derived past each keychain's reveal frontier and indexed against.
///
/// Wide because 0.3 revealed a change address on every `fee()` call, so a transaction it built
/// could pay change well past the frontier its database kept. Those transactions are already
/// stored — we own one of their spent prevouts — so attributing the change is purely a matter of
/// having derived far enough to recognise the script, and `KeychainTxOutIndex` reveals to any index
/// it matches. A wallet whose change is stranded that way heals itself the next time its key is
/// touched.
///
/// This is a parameter, not a limit anything depends on. Nothing records that a database has been
/// searched, so raising this repairs every affected wallet on its next launch. Cost is linear:
/// roughly this many secp derivations per keychain, once per session.
///
/// Deliberately NOT the chain source's subscription window — see `SUBSCRIPTION_LOOKAHEAD`.
/// Deriving a script is local work against our own keys; subscribing to one hands it to a server.
pub const LOOKAHEAD: u32 = 1000;

pub type KeychainId = (MasterAppkey, BitcoinAccountKeychain);
pub type WalletIndexer = KeychainTxOutIndex<KeychainId>;
pub type WalletIndexedTxGraph =
    indexed_tx_graph::IndexedTxGraph<ConfirmationBlockTime, WalletIndexer>;
pub type WalletIndexedTxGraphChangeSet =
    indexed_tx_graph::ChangeSet<ConfirmationBlockTime, keychain_txout::ChangeSet>;

/// Wallet that manages all the frostsnap keys on the same network in a single transaction graph
pub struct CoordSuperWallet {
    pub(super) tx_graph: Persisted<WalletIndexedTxGraph>,
    pub(super) chain: Persisted<local_chain::LocalChain>,
    pub(super) chain_client: ChainClient,
    pub network: bitcoin::Network,
    pub(super) db: Arc<Mutex<rusqlite::Connection>>,
}

impl CoordSuperWallet {
    pub fn load_or_init(
        db: Arc<Mutex<rusqlite::Connection>>,
        network: bitcoin::Network,
        chain_client: ChainClient,
    ) -> anyhow::Result<Self> {
        Self::load_or_init_with_lookahead(db, network, chain_client, LOOKAHEAD)
    }

    /// [`Self::load_or_init`] with the derived window named rather than assumed.
    ///
    /// How far a database is searched is a parameter, not something the database remembers, and the
    /// persistence layer is the wrong place to decide it. Keeping it an argument is also what makes
    /// the promise checkable: raising [`LOOKAHEAD`] in a later release finds what the smaller one
    /// passed over.
    pub fn load_or_init_with_lookahead(
        db: Arc<Mutex<rusqlite::Connection>>,
        network: bitcoin::Network,
        chain_client: ChainClient,
        lookahead: u32,
    ) -> anyhow::Result<Self> {
        event!(
            Level::INFO,
            network = network.to_string(),
            "initializing super wallet"
        );
        let mut db_ = db.lock().unwrap();
        let tx_graph = Persisted::new(&mut *db_, lookahead)
            .context("loading transaction from the database")?;
        let chain = Persisted::new(
            &mut *db_,
            bitcoin::constants::genesis_block(network).block_hash(),
        )
        .context("loading chain from database")?;
        drop(db_);

        Ok(Self {
            tx_graph,
            chain,
            chain_client,
            db,
            network,
        })
    }

    /// Get the local chain tip.
    pub fn chain_tip(&self) -> CheckPoint {
        self.chain.tip()
    }

    /// Transaction cache for the chain client.
    pub fn tx_cache(&self) -> impl Iterator<Item = (Txid, Arc<bitcoin::Transaction>)> + '_ {
        self.tx_graph
            .graph()
            .full_txs()
            .map(|tx_node| (tx_node.txid, tx_node.tx))
    }

    pub fn anchor_cache(
        &self,
    ) -> impl Iterator<Item = ((Txid, BlockHash), ConfirmationBlockTime)> + '_ {
        self.tx_graph
            .graph()
            .all_anchors()
            .iter()
            .flat_map(|(&txid, anchors)| {
                anchors
                    .iter()
                    .map(move |&anchor| ((txid, anchor.block_id.hash), anchor))
            })
    }

    /// Which of our scripts each transaction we hold appears in the history of, keyed the way
    /// the server keys it.
    ///
    /// This is what lets an eviction be noticed: the chain source spots one by missing a txid
    /// from the history the server now reports, so it can only spot the ones it knew about. It
    /// is not persisted with the rest of the cache — this is the wallet data it is rebuilt from
    /// on every startup, and without it an eviction that happened while we were disconnected is
    /// never detected.
    ///
    /// Mirrors Electrum's definition of a script's history: a transaction is in it if it pays
    /// the script or spends an output of it.
    pub fn spk_txid_cache(&self) -> impl Iterator<Item = (ScriptBuf, Txid)> + '_ {
        let graph = self.tx_graph.graph();
        let index = &self.tx_graph.index;
        graph.full_txs().flat_map(move |tx_node| {
            let txid = tx_node.txid;
            let paid_to = tx_node.tx.output.iter().map(|txout| &txout.script_pubkey);
            let spent_from = tx_node
                .tx
                .input
                .iter()
                .filter_map(|txin| graph.get_txout(txin.previous_output))
                .map(|txout| &txout.script_pubkey);
            paid_to
                .chain(spent_from)
                .filter(|spk| index.index_of_spk((*spk).clone()).is_some())
                .map(|spk| (spk.clone(), txid))
                .collect::<Vec<_>>()
        })
    }

    /// How far past a frontier the indexer derives. Not what the chain source subscribes to — see
    /// `SUBSCRIPTION_LOOKAHEAD`.
    pub fn lookahead(&self) -> u32 {
        self.tx_graph.index.lookahead()
    }

    pub fn get_tx(&self, txid: Txid) -> Option<Arc<bitcoin::Transaction>> {
        self.tx_graph.graph().get_tx(txid)
    }

    pub fn get_txout(&self, outpoint: OutPoint) -> Option<bitcoin::TxOut> {
        self.tx_graph.graph().get_txout(outpoint).cloned()
    }

    pub fn get_prevouts(
        &self,
        outpoints: impl Iterator<Item = OutPoint>,
    ) -> HashMap<OutPoint, TxOut> {
        outpoints
            .into_iter()
            .filter_map(|op| Some((op, self.get_txout(op)?)))
            .collect()
    }

    /// The derivation this wallet reaches `spk` by, or `None` if it does not reach it.
    /// The keychain is half the answer: receive #5 and change #5 are different scripts,
    /// and an index alone cannot say which one was found.
    pub fn spk_path(
        &self,
        master_appkey: MasterAppkey,
        spk: ScriptBuf,
    ) -> Option<BitcoinBip32Path> {
        let &((key, account_keychain), index) = self.tx_graph.index.index_of_spk(spk)?;
        (key == master_appkey).then(|| BitcoinBip32Path {
            account_keychain,
            index: NormalIndex::new(index)
                .expect("bdk derived this spk, so its index is a normal child"),
        })
    }

    fn descriptors_for_key(
        approot: MasterAppkey,
        network: bitcoin::NetworkKind,
    ) -> Vec<(BitcoinAccountKeychain, Descriptor<DescriptorPublicKey>)> {
        [
            BitcoinAccountKeychain::external(),
            BitcoinAccountKeychain::internal(),
        ]
        .into_iter()
        .zip(
            //XXX: this logic is very brittle and implicit with respect to accounts
            super::multi_x_descriptor_for_account(approot, BitcoinAccount::default(), network)
                .into_single_descriptors()
                .expect("should be well formed"),
        )
        .collect()
    }

    pub(super) fn lazily_initialize_key(&mut self, master_appkey: MasterAppkey) {
        if self
            .tx_graph
            .index
            .get_descriptor((master_appkey, BitcoinAccountKeychain::external()))
            .is_none()
        {
            // bdk does not persist the txout index and is not going to, so attribution has to be
            // rebuilt from the stored transactions. That part is permanent; doing it on whichever
            // call happens to touch a key first is not.
            //
            // FIXME: replace this with a formal initialisation step, so a key's descriptors and
            // index are built at a defined point rather than as a side effect of the first reader.
            let mut db = self.db.lock().unwrap();
            self.tx_graph
                .mutate(&mut db, |tx_graph| {
                    for (account_keychain, descriptor) in
                        Self::descriptors_for_key(master_appkey, self.network.into())
                    {
                        tx_graph
                            .index
                            .insert_descriptor((master_appkey, account_keychain), descriptor)
                            .expect("two keychains must not have the same spks");
                    }
                    // insert_descriptor doesn't trigger a re-index of all the
                    // transactions so we trigger it manually. You might think
                    // that this is always a NOOP but because we've changed the
                    // lookahead in versions, the first time it's loaded in a
                    // new wallet it can find things.
                    let changeset = tx_graph.reindex();
                    Ok(((), changeset))
                })
                .expect("rebuilding the index cannot fail");
            drop(db);

            // again in case it found something tell electrum that it's got to start looking further.
            self.resync_monitoring();
        }
    }

    /// Tell the chain source where every keychain we know about now ends.
    ///
    /// We actually have a larger lookahead locally than we give to the electrum
    /// client. This saves server bandwidth but if we find something further out
    /// than electrum is looking we have to tell it about it.
    ///
    /// Free to over-call, at the message level and not merely the state level: a tracked window only
    /// ever widens, and only scripts it does not already hold become subscribe requests. A wallet
    /// with nothing far out generates no traffic.
    fn resync_monitoring(&self) {
        for (keychain_id, _) in self.tx_graph.index.keychains() {
            let next_index = self
                .tx_graph
                .index
                .last_revealed_index(keychain_id)
                .map_or(0, |lr| lr + 1);
            self.chain_client.monitor_keychain(keychain_id, next_index);
        }
    }

    pub fn list_addresses(&mut self, master_appkey: MasterAppkey) -> Vec<AddressInfo> {
        self.lazily_initialize_key(master_appkey);
        let keychain = BitcoinAccountKeychain::external();
        let (final_address_index, _) = self
            .tx_graph
            .index
            .next_index((master_appkey, keychain))
            .expect("keychain exists");
        (0..=final_address_index)
            .rev()
            .map(|i| {
                self.address_info(
                    master_appkey,
                    BitcoinBip32Path {
                        account_keychain: keychain,
                        index: NormalIndex::new(i).expect(
                            "i is bounded by next_index, which bdk saturates at BIP32_MAX_INDEX",
                        ),
                    },
                )
            })
            .collect()
    }

    pub fn address(&mut self, master_appkey: MasterAppkey, index: u32) -> Option<AddressInfo> {
        self.lazily_initialize_key(master_appkey);
        let keychain = BitcoinAccountKeychain::external();
        Some(self.address_info(
            master_appkey,
            BitcoinBip32Path {
                account_keychain: keychain,
                index: NormalIndex::new(index)?,
            },
        ))
    }

    fn address_info(&self, master_appkey: MasterAppkey, path: BitcoinBip32Path) -> AddressInfo {
        let keychain = (master_appkey, path.account_keychain);
        let used = self.tx_graph.index.is_used(keychain, path.index.to_u32());
        let revealed =
            Some(path.index.to_u32()) <= self.tx_graph.index.last_revealed_index(keychain);
        let spk = super::peek_spk(master_appkey, path);
        AddressInfo {
            index: path.index.to_u32(),
            address: bitcoin::Address::from_script(&spk, self.network).expect("has address form"),
            external: path.account_keychain == BitcoinAccountKeychain::external(),
            used,
            revealed,
            derivation_path: path.path_segments_from_bitcoin_appkey().collect(),
        }
    }

    pub fn next_address(&mut self, master_appkey: MasterAppkey) -> AddressInfo {
        self.lazily_initialize_key(master_appkey);
        let keychain = BitcoinAccountKeychain::external();
        let (index, _) = self
            .tx_graph
            .index
            .next_index((master_appkey, keychain))
            .expect("keychain exists");

        self.address_info(
            master_appkey,
            BitcoinBip32Path {
                account_keychain: keychain,
                index: NormalIndex::new(index)
                    .expect("bdk saturates next_index at BIP32_MAX_INDEX"),
            },
        )
    }

    pub fn mark_address_shared(
        &mut self,
        master_appkey: MasterAppkey,
        derivation_index: u32,
    ) -> Result<bool> {
        self.lazily_initialize_key(master_appkey);
        let keychain = BitcoinAccountKeychain::external();
        let mut db = self.db.lock().unwrap();
        let already_shared = self.tx_graph.mutate(&mut db, |tx_graph| {
            let (_, changeset) = tx_graph
                .index
                .reveal_to_target((master_appkey, keychain), derivation_index)
                .ok_or(anyhow!("keychain doesn't exist"))?;

            Ok((changeset.is_empty(), changeset))
        })?;
        drop(db);
        Ok(already_shared)
    }

    pub fn search_for_address(
        &self,
        master_appkey: MasterAppkey,
        address_str: String,
        start: u32,
        stop: u32,
    ) -> Option<AddressInfo> {
        let account_descriptors = multi_x_descriptor_for_account(
            master_appkey,
            BitcoinAccount::default(),
            self.network.into(),
        )
        .into_single_descriptors()
        .ok()?;
        let target_address = bitcoin::Address::from_str(&address_str)
            .ok()?
            .require_network(self.network)
            .ok()?;

        let found_address_derivation = {
            (start..stop).find_map(|i| {
                account_descriptors.iter().find_map(|descriptor| {
                    let derived = descriptor.at_derivation_index(i).ok()?;
                    let address = derived.address(self.network).ok()?;
                    if address == target_address {
                        // FIXME: this should get the derivation path from the descriptor itself
                        let external = account_descriptors[0] == *descriptor;
                        let keychain = if external {
                            BitcoinAccountKeychain::external()
                        } else {
                            BitcoinAccountKeychain::internal()
                        };

                        Some(self.address_info(
                            master_appkey,
                            BitcoinBip32Path {
                                account_keychain: keychain,
                                index: NormalIndex::new(i)?,
                            },
                        ))
                    } else {
                        None
                    }
                })
            })
        };
        found_address_derivation
    }

    pub fn list_transactions(&mut self, master_appkey: MasterAppkey) -> Vec<Transaction> {
        self.lazily_initialize_key(master_appkey);
        let chain_tip = self.chain.tip();
        let chain_tip_height = chain_tip.height();
        // bdk's canonical order is topological (spend-depth), not by time, so reverse it to
        // get child-before-parent, then sort newest-first by chain position (pending first,
        // then confirmed by height). Stable sort keeps the child-before-parent tiebreak.
        let mut canonical = self
            .tx_graph
            .graph()
            .list_ordered_canonical_txs(
                self.chain.as_ref(),
                chain_tip.block_id(),
                CanonicalizationParams::default(),
            )
            .collect::<Vec<_>>();
        canonical.reverse();
        canonical.sort_by_key(|tx| std::cmp::Reverse(tx.chain_position));
        canonical
            .into_iter()
            .filter_map(|canonical_tx| {
                let inner = canonical_tx.tx_node.tx.clone();
                let txid = canonical_tx.tx_node.txid;
                let confirmation_time = match canonical_tx.chain_position {
                    ChainPosition::Confirmed { anchor, .. } => Some(ConfirmationTime {
                        height: anchor.block_id.height,
                        time: anchor.confirmation_time,
                    }),
                    _ => None,
                };
                let confirmations = confirmation_time.as_ref().map_or(0, |confirmation| {
                    confirmation.confirmations_at_tip(chain_tip_height)
                });
                let last_seen = canonical_tx.tx_node.last_seen;
                let prevouts =
                    self.get_prevouts(inner.input.iter().map(|txin| txin.previous_output));
                let is_mine = inner
                    .output
                    .iter()
                    .chain(prevouts.values())
                    .filter_map(|txout| {
                        let spk = txout.script_pubkey.clone();
                        self.tx_graph
                            .index
                            .index_of_spk(spk.clone())
                            .filter(|((key, _), _)| *key == master_appkey)
                            .map(|((_, _), index)| (spk, *index))
                    })
                    .collect::<HashMap<ScriptBuf, u32>>();
                if is_mine.is_empty() {
                    None
                } else {
                    Some(Transaction {
                        inner,
                        txid,
                        confirmation_time,
                        confirmations,
                        last_seen,
                        prevouts,
                        is_mine,
                    })
                }
            })
            .collect()
    }

    pub fn apply_update(
        &mut self,
        update: bdk_electrum_streaming::Update<KeychainId>,
    ) -> Result<bool> {
        let mut db = self.db.lock().unwrap();
        let changed = self
            .tx_graph
            .multi(&mut self.chain)
            .mutate(&mut db, |tx_graph, chain| {
                let chain_changeset = match update.chain_update {
                    Some(update) => chain.apply_update(update)?,
                    None => local_chain::ChangeSet::default(),
                };
                let indexer_changeset = tx_graph
                    .index
                    .reveal_to_target_multi(&update.last_active_indices);
                let mut changeset = tx_graph.apply_update(update.tx_update);
                changeset.indexer.merge(indexer_changeset);
                let changed = !(chain_changeset.is_empty() && changeset.is_empty());
                Ok((changed, (changeset, chain_changeset)))
            })?;
        drop(db);
        // A frontier can only move by revealing, which lands in the changeset, so `changed` covers
        // every case worth signalling.
        if changed {
            self.resync_monitoring();
        }
        Ok(changed)
    }

    pub fn reconnect(&mut self) {
        self.chain_client.reconnect()
    }

    pub fn calculate_avaliable_value(
        &mut self,
        master_appkey: MasterAppkey,
        target_addresses: impl IntoIterator<Item = bitcoin::Address>,
        feerate: f32,
        effective_only: bool,
    ) -> i64 {
        self.lazily_initialize_key(master_appkey);
        use bdk_coin_select::{
            Candidate, CoinSelector, Drain, FeeRate, Target, TargetFee, TargetOutputs,
            TR_KEYSPEND_TXIN_WEIGHT,
        };

        let feerate = FeeRate::from_sat_per_vb(feerate);
        let target = Target {
            fee: TargetFee::from_feerate(feerate),
            outputs: TargetOutputs::fund_outputs(target_addresses.into_iter().map(|addr| {
                let txo = bitcoin::TxOut {
                    script_pubkey: addr.script_pubkey(),
                    value: Amount::ZERO,
                };
                (txo.weight().to_wu(), 0)
            })),
        };
        let candidates = self
            .tx_graph
            .graph()
            .filter_chain_unspents(
                self.chain.as_ref(),
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
                self.tx_graph
                    .index
                    .keychain_outpoints_in_range(Self::key_index_range(master_appkey)),
            )
            .map(|(_path, utxo)| Candidate {
                input_count: 1,
                value: utxo.txout.value.to_sat(),
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                is_segwit: true,
            })
            .collect::<Vec<_>>();

        let mut cs = CoinSelector::new(&candidates);
        if effective_only {
            cs.select_all_effective(feerate);
        } else {
            cs.select_all();
        }
        cs.excess(target, Drain::NONE)
    }

    pub(super) fn key_index_range(
        master_appkey: MasterAppkey,
    ) -> impl RangeBounds<(MasterAppkey, BitcoinAccountKeychain)> {
        (master_appkey, BitcoinAccountKeychain::external())
            ..=(master_appkey, BitcoinAccountKeychain::internal())
    }

    pub fn fee(&self, tx: &bitcoin::Transaction) -> Result<u64> {
        let fee = self.tx_graph.graph().calculate_fee(tx)?;
        Ok(fee.to_sat())
    }

    pub fn broadcast_success(&mut self, tx: bitcoin::Transaction) {
        // We do our best here, if it fails to persist we should recover from this eventually
        let res = self
            .tx_graph
            .mutate(&mut *self.db.lock().unwrap(), |tx_graph| {
                let mut changeset = tx_graph.insert_seen_at(
                    tx.compute_txid(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                );
                changeset.merge(tx_graph.insert_tx(tx));
                Ok(((), changeset))
            });

        if let Err(e) = res {
            event!(
                Level::ERROR,
                error = e.to_string(),
                "failed to persist broadcast"
            );
        }
    }
}

pub use frostsnap_core::psbt::PsbtValidationError;

#[derive(Clone, Debug)]
pub struct AddressInfo {
    pub index: u32,
    pub address: bitcoin::Address,
    pub external: bool,
    pub used: bool,
    pub revealed: bool,
    pub derivation_path: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub inner: Arc<bitcoin::Transaction>,
    pub txid: Txid,

    pub confirmation_time: Option<ConfirmationTime>,
    /// Confirmations against the same chain snapshot that canonicalized this transaction.
    pub confirmations: u32,
    pub last_seen: Option<u64>,

    pub prevouts: HashMap<OutPoint, TxOut>,
    /// Maps owned script pubkeys to their derivation index.
    pub is_mine: HashMap<ScriptBuf, u32>,
}

#[derive(Clone, Debug)]
pub struct ConfirmationTime {
    pub height: u32,
    pub time: u64,
}

impl ConfirmationTime {
    fn confirmations_at_tip(&self, chain_tip_height: u32) -> u32 {
        chain_tip_height
            .checked_sub(self.height)
            .map_or(0, |depth| depth + 1)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::key::{Secp256k1, TweakedPublicKey};
    use frostsnap_core::{schnorr_fun::fun::Point, tweak::AppTweak};

    #[test]
    fn confirmation_count_is_derived_from_the_wallet_snapshot_tip() {
        let confirmation = ConfirmationTime {
            height: 100,
            time: 0,
        };

        assert_eq!(confirmation.confirmations_at_tip(100), 1);
        assert_eq!(confirmation.confirmations_at_tip(105), 6);
        assert_eq!(confirmation.confirmations_at_tip(99), 0);
    }

    #[test]
    fn wallet_descriptors_match_our_tweaking() {
        let master_appkey =
            MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
        let descriptors =
            CoordSuperWallet::descriptors_for_key(master_appkey, bitcoin::NetworkKind::Main);

        let (account_keychain, external_descriptor) = &descriptors[0];
        let xonly = AppTweak::Bitcoin(BitcoinBip32Path {
            account_keychain: *account_keychain,
            index: NormalIndex::new(42).unwrap(),
        })
        .derive_xonly_key(&master_appkey.to_xpub());

        let definite_descriptor = external_descriptor.at_derivation_index(42).unwrap();
        definite_descriptor
            .derived_descriptor(&Secp256k1::default())
            .unwrap()
            .to_string();

        let desc_spk = definite_descriptor.script_pubkey();

        assert_eq!(
            desc_spk,
            bitcoin::ScriptBuf::new_p2tr_tweaked(TweakedPublicKey::dangerous_assume_tweaked(
                xonly.into()
            )),
        );
    }
}
