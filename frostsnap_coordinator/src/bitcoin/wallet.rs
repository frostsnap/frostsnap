use super::{chain_sync::ChainClient, multi_x_descriptor_for_account};
use crate::persist::Persisted;
use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self, Amount, BlockHash, OutPoint, ScriptBuf, TxOut, Txid},
    indexed_tx_graph::{self},
    indexer::keychain_txout::{self, KeychainTxOutIndex},
    local_chain,
    miniscript::{Descriptor, DescriptorPublicKey},
    CanonicalizationParams, ChainPosition, CheckPoint, ConfirmationBlockTime, Indexer, Merge,
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
    chain_client: ChainClient,
    pub network: bitcoin::Network,
    pub(super) db: Arc<Mutex<rusqlite::Connection>>,
}

impl CoordSuperWallet {
    pub fn load_or_init(
        db: Arc<Mutex<rusqlite::Connection>>,
        network: bitcoin::Network,
        chain_client: ChainClient,
    ) -> anyhow::Result<Self> {
        event!(
            Level::INFO,
            network = network.to_string(),
            "initializing super wallet"
        );
        let mut db_ = db.lock().unwrap();
        let tx_graph =
            Persisted::new(&mut *db_, ()).context("loading transaction from the database")?;
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

    pub fn is_spk_mine(&self, master_appkey: MasterAppkey, spk: ScriptBuf) -> bool {
        self.spk_index(master_appkey, spk).is_some()
    }

    pub fn spk_index(&self, master_appkey: MasterAppkey, spk: ScriptBuf) -> Option<u32> {
        self.tx_graph
            .index
            .index_of_spk(spk)
            .and_then(|((key, _), index)| (*key == master_appkey).then_some(*index))
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
            for (account_keychain, descriptor) in
                Self::descriptors_for_key(master_appkey, self.network.into())
            {
                let keychain_id = (master_appkey, account_keychain);
                self.tx_graph
                    .MUTATE_NO_PERSIST()
                    .index
                    .insert_descriptor(keychain_id, descriptor)
                    .expect("two keychains must not have the same spks");
                // the spk tracker already applies lookahead on top of this
                let next_index = self
                    .tx_graph
                    .index
                    .last_revealed_index(keychain_id)
                    .map_or(0, |lr| lr + 1);
                self.chain_client.monitor_keychain(keychain_id, next_index);
            }
            let all_txs = self
                .tx_graph
                .graph()
                .full_txs()
                .map(|tx| tx.tx.clone())
                .collect::<Vec<_>>();
            // FIXME: This should be done by BDK automatically in a version soon.
            // FIXME: We want a high enough last-derived-index before doing indexing otherwise we
            // may misindex some txs.
            for tx in &all_txs {
                let _ = self.tx_graph.MUTATE_NO_PERSIST().index.index_tx(tx);
            }
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
                        index: revealed_index(i),
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
            self.tx_graph.index.last_revealed_index(keychain) <= Some(path.index.to_u32());
        let spk = super::peek_spk(master_appkey, path);
        AddressInfo {
            index: path.index.to_u32(),
            address: bitcoin::Address::from_script(&spk, self.network).expect("has address form"),
            external: true,
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
                index: revealed_index(index),
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
        self.tx_graph.mutate(&mut db, |tx_graph| {
            let (_, changeset) = tx_graph
                .index
                .reveal_to_target((master_appkey, keychain), derivation_index)
                .ok_or(anyhow!("keychain doesn't exist"))?;

            Ok((changeset.is_empty(), changeset))
        })
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
        // bdk's canonical order is topological (spend-depth), not by time, so reverse it to
        // get child-before-parent, then sort newest-first by chain position (pending first,
        // then confirmed by height). Stable sort keeps the child-before-parent tiebreak.
        let mut canonical = self
            .tx_graph
            .graph()
            .list_ordered_canonical_txs(
                self.chain.as_ref(),
                self.chain.tip().block_id(),
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

pub use super::psbt_template::PsbtValidationError;

/// bdk stops revealing at `BIP32_MAX_INDEX`, so any index it hands back is a normal child.
pub(super) fn revealed_index(index: u32) -> NormalIndex {
    NormalIndex::new(index).expect("bdk never reveals past BIP32_MAX_INDEX")
}

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

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::key::{Secp256k1, TweakedPublicKey};
    use frostsnap_core::{schnorr_fun::fun::Point, tweak::AppTweak};

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
