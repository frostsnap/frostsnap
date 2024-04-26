use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self, bip32, secp256k1, Amount, ScriptBuf, SignedAmount, Transaction},
    indexed_tx_graph,
    keychain::{self, KeychainTxOutIndex},
    local_chain::{self, LocalChain},
    miniscript::{
        descriptor::{DescriptorXKey, Tr, Wildcard},
        Descriptor, DescriptorPublicKey,
    },
    spk_client, tx_graph, Append, ChainPosition, ConfirmationTimeHeightAnchor,
};
use flutter_rust_bridge::RustOpaque;
use frostsnap_coordinator::frostsnap_core::{
    self,
    message::{BitcoinTransactionSignTask, EncodedSignature},
    schnorr_fun::{frost::FrostKey, fun::marker::Normal},
    tweak::TweakableKey,
    CoordinatorFrostKey, FrostKeyExt, KeyId,
};
use llsdb::{IndexHandle, LinkedList, LlsDb};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    ops::RangeBounds,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};

use crate::{
    api::{self, UnsignedTx},
    chain_sync::SyncRequest,
    persist_core::PersistCore,
};

pub type TxGraphChangeSet = tx_graph::ChangeSet<ConfirmationTimeHeightAnchor>;
pub type ChainChangeSet = local_chain::ChangeSet;
pub type WalletIndexedTxGraph = indexed_tx_graph::IndexedTxGraph<
    ConfirmationTimeHeightAnchor,
    KeychainTxOutIndex<(KeyId, Keychain)>,
>;
pub type WalletIndexedTxGraphChangeSet = indexed_tx_graph::ChangeSet<
    ConfirmationTimeHeightAnchor,
    keychain::ChangeSet<(KeyId, Keychain)>,
>;

// Flutter rust bridge is annoyed if I call this `Wallet`
pub struct _Wallet {
    graph: WalletIndexedTxGraph,
    chain: LocalChain,
    pub network: bitcoin::Network,
    db: Arc<Mutex<LlsDb<File>>>,
    chain_list_handle: LinkedList<bincode::serde::Compat<ChainChangeSet>>,
    tx_graph_list_handle: LinkedList<bincode::serde::Compat<TxGraphChangeSet>>,
    /// Which spks have been revealed for which descriptors
    spk_revelation_list_handle:
        LinkedList<bincode::serde::Compat<BTreeMap<bdk_chain::DescriptorId, u32>>>,
    persist_core: IndexHandle<PersistCore>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Keychain {
    External = 0,
    Internal = 1,
}

impl _Wallet {
    pub fn load_or_init(
        db: Arc<Mutex<LlsDb<File>>>,
        network: bitcoin::Network,
        persist_core: IndexHandle<PersistCore>,
    ) -> anyhow::Result<Self> {
        event!(Level::INFO, "initializing wallet");
        let mut db_ = db.lock().unwrap();
        let (
            chain,
            graph,
            network,
            tx_graph_list_handle,
            chain_list_handle,
            spk_revelation_list_handle,
        ) = db_
            .execute(|tx| {
                // store the graph and chain data in differnet lists so they're easier to work with e.g.
                // can delete all tx data without deleting chain data and vis versa
                let chain_list_handle = tx
                    .take_list::<bincode::serde::Compat<crate::wallet::ChainChangeSet>>(
                        "wallet/chain",
                    )
                    .context("loading chain list")?;
                let tx_graph_list_handle = tx
                    .take_list::<bincode::serde::Compat<crate::wallet::TxGraphChangeSet>>(
                        "wallet/tx_graph",
                    )
                    .context("loading tx data list")?;

                let spk_revelation_list_handle = tx
                    .take_list::<bincode::serde::Compat<BTreeMap<bdk_chain::DescriptorId, u32>>>(
                        "wallet/keychain",
                    )
                    .context("loading keychain list")?;
                let chain_list = chain_list_handle.api(&tx);
                let tx_list = tx_graph_list_handle.api(&tx);
                let spk_revelation = spk_revelation_list_handle.api(&tx);
                let (chain, graph) = if chain_list.is_empty() {
                    event!(
                        Level::INFO,
                        "seting up wallet for the first time -- writing to disk"
                    );
                    let genesis_hash = bitcoin::constants::genesis_block(network).block_hash();
                    let (chain, chain_changeset) = LocalChain::from_genesis_hash(genesis_hash);

                    let graph = Default::default();
                    chain_list.push(&bincode::serde::Compat(chain_changeset))?;
                    (chain, graph)
                } else {
                    event!(Level::INFO, "loading existing wallet from disk");
                    event!(Level::INFO, "loading chain data");
                    let chain_data = {
                        // unfortunately, local_chain::ChangeSet is not monotone so the order matters,
                        // so we have to reverse the iterator
                        let mut changes = VecDeque::new();
                        for changeset in chain_list.iter() {
                            changes.push_front(changeset?)
                        }
                        let mut full_changeset = crate::wallet::ChainChangeSet::default();
                        for mut changeset in changes {
                            full_changeset.append(&mut changeset.0);
                        }
                        full_changeset
                    };

                    event!(Level::INFO, "loading transaction data");
                    let tx_data = tx_list.iter().try_fold(
                        TxGraphChangeSet::default(),
                        |mut acc, next| -> anyhow::Result<_> {
                            acc.append(next?.0);
                            Ok(acc)
                        },
                    )?;

                    let mut graph = WalletIndexedTxGraph::default();

                    let persisted_frost_keys = tx
                        .take_index(persist_core)
                        .coord_frost_keys()
                        .context("reading persisted frost keys")?;

                    for coord_frost_key in persisted_frost_keys {
                        let key_id = coord_frost_key.key_id();
                        for (keychain, descriptor) in
                            Self::get_descriptors(&coord_frost_key.frost_key(), network)
                        {
                            let _dont_persist_descriptors = graph
                                .index
                                .insert_descriptor((key_id, keychain), descriptor);
                        }
                    }

                    let revelation_changes = spk_revelation.iter().try_fold(
                        keychain::ChangeSet::default(),
                        |mut acc, next| -> anyhow::Result<_> {
                            acc.append(keychain::ChangeSet {
                                last_revealed: next?.0,
                                ..Default::default()
                            });
                            Ok(acc)
                        },
                    )?;

                    graph.apply_changeset(indexed_tx_graph::ChangeSet {
                        graph: tx_data,
                        indexer: revelation_changes,
                    });
                    let chain = LocalChain::from_changeset(chain_data)?;
                    (chain, graph)
                };
                Ok((
                    chain,
                    graph,
                    network,
                    tx_graph_list_handle,
                    chain_list_handle,
                    spk_revelation_list_handle,
                ))
            })
            .context("Initializing wallet")?;

        event!(Level::INFO, "wallet initialization finished");

        drop(db_);
        Ok(Self {
            db,
            chain,
            graph,
            network,
            tx_graph_list_handle,
            chain_list_handle,
            spk_revelation_list_handle,
            persist_core,
        })
    }

    fn get_descriptors(
        frost_key: &FrostKey<Normal>,
        network: bitcoin::Network,
    ) -> Vec<(Keychain, Descriptor<DescriptorPublicKey>)> {
        let secp = secp256k1::Secp256k1::verification_only();

        let (app_key, chaincode) =
            frost_key.app_tweak_and_expand(frostsnap_core::tweak::AppTweakKind::Bitcoin);
        let root_bitcoin_xpub = frostsnap_core::tweak::Xpub::new(app_key, chaincode).xpub(network);

        [Keychain::External, Keychain::Internal]
            .into_iter()
            .map(|keychain| {
                let child_xpub = root_bitcoin_xpub
                    .ckd_pub(
                        &secp,
                        bip32::ChildNumber::Normal {
                            index: keychain as u32,
                        },
                    )
                    .unwrap();
                let desc_key = DescriptorPublicKey::XPub(DescriptorXKey {
                    origin: Some((
                        root_bitcoin_xpub.fingerprint(),
                        bip32::DerivationPath::master(),
                    )),
                    xkey: root_bitcoin_xpub,
                    derivation_path: bip32::DerivationPath::master().child(child_xpub.child_number),
                    wildcard: Wildcard::Unhardened,
                });
                let tr = Tr::new(desc_key, None).expect("infallible since it's None");
                (keychain, Descriptor::Tr(tr))
            })
            .collect()
    }

    fn lazily_initialize_key(&mut self, key_id: KeyId) -> Result<()> {
        if self
            .graph
            .index
            .get_descriptor(&(key_id, Keychain::External))
            .is_none()
        {
            let found = self
                .get_frost_key(key_id)?
                .ok_or(anyhow!("key {key_id} doesn't exist in database"))?;
            for (keychain, descriptor) in Self::get_descriptors(&found.frost_key(), self.network) {
                let _intentionally_ignore_saving_descriptors = self
                    .graph
                    .index
                    .insert_descriptor((key_id, keychain), descriptor);
            }
        }
        Ok(())
    }

    fn get_frost_key(&self, key_id: KeyId) -> Result<Option<CoordinatorFrostKey>> {
        let coord_frost_keys = self
            .db
            .lock()
            .unwrap()
            .execute(|tx| tx.take_index(self.persist_core).coord_frost_keys())?;
        let found = coord_frost_keys
            .into_iter()
            .find(|coord_frost_key| coord_frost_key.frost_key().key_id() == key_id);

        Ok(found)
    }

    pub fn list_addresses(&self, key_id: KeyId) -> Vec<api::Address> {
        self.graph
            .index
            .revealed_keychain_spks(&(key_id, Keychain::External))
            .rev()
            .map(|(i, spk)| api::Address {
                index: i,
                address_string: bitcoin::Address::from_script(spk, self.network)
                    .expect("has address form")
                    .to_string(),
                used: self.graph.index.is_used((key_id, Keychain::External), i),
            })
            .collect()
    }

    pub fn next_address(&mut self, key_id: KeyId) -> Result<api::Address> {
        // We don't know in the wallet when a new key has been created so we need to lazily
        // initialze the wallet with it when we first get asked for an address.
        self.lazily_initialize_key(key_id)?;

        if let Some(((index, spk), changeset)) = self
            .graph
            .index
            .reveal_next_spk(&(key_id, Keychain::External))
        {
            let spk = spk.to_owned();
            self.db.lock().unwrap().execute(|tx| {
                self.consume_spk_revelation_change(tx, changeset)?;
                Ok(())
            })?;
            // TODO: There should be a way of unrevealing index if we fail to persist:
            // https://github.com/bitcoindevkit/bdk/issues/1322
            Ok(api::Address {
                index,
                address_string: bitcoin::Address::from_script(&spk, self.network)
                    .expect("has address form")
                    .to_string(),
                used: self
                    .graph
                    .index
                    .is_used((key_id, Keychain::External), index),
            })
        } else {
            Err(anyhow!("no more addresess on this keychain"))?
        }
    }

    fn consume_spk_revelation_change<K>(
        &self,
        tx: &mut llsdb::Transaction<impl llsdb::Backend>,
        changeset: keychain::ChangeSet<K>,
    ) -> Result<()> {
        let list = self.spk_revelation_list_handle.api(tx);

        if !changeset.last_revealed.is_empty() {
            // note carefully that we ignore the changeset's added descriptor changes since we don't
            // want to store these for BDK since we cand derive them from our frost keys
            list.push(&bincode::serde::Compat(changeset.last_revealed))?;
        }

        Ok(())
    }

    pub fn list_transactions(&mut self, key_id: KeyId) -> Vec<api::Transaction> {
        let mut txs = self
            .graph
            .graph()
            .list_chain_txs(&self.chain, self.chain.tip().block_id())
            .collect::<Vec<_>>();

        txs.sort_unstable_by_key(|tx| core::cmp::Reverse(tx.chain_position));
        txs.into_iter()
            .filter_map(|canonical_tx| {
                let confirmation_time = match canonical_tx.chain_position {
                    ChainPosition::Confirmed(conf_time) => Some(api::ConfirmationTime {
                        height: conf_time.confirmation_height,
                        time: conf_time.confirmation_time,
                    }),
                    _ => None,
                };
                let net_value = self
                    .graph
                    .index
                    .net_value(&canonical_tx.tx_node.tx, Self::key_index_range(key_id));

                if net_value == SignedAmount::ZERO {
                    return None;
                }
                Some(api::Transaction {
                    inner: RustOpaque::new((*canonical_tx.tx_node.tx).clone()),
                    confirmation_time,
                    net_value: net_value.to_sat(),
                })
            })
            .collect()
    }

    pub fn sync_txs(&self, txids: Vec<bitcoin::Txid>) -> SyncRequest {
        SyncRequest::from_chain_tip(self.chain.tip()).chain_txids(txids)
    }

    pub fn start_sync(&self, key_id: KeyId) -> SyncRequest {
        // We want to sync all spks for now!
        let interesting_spks = self
            .graph
            .index
            .revealed_spks(Self::key_index_range(key_id))
            .map(|(_, spk)| spk.to_owned())
            .collect::<Vec<_>>();

        SyncRequest::from_chain_tip(self.chain.tip()).chain_spks(interesting_spks)
    }

    pub fn finish_sync(
        &mut self,
        update: spk_client::SyncResult<ConfirmationTimeHeightAnchor>,
    ) -> Result<bool> {
        let indexed_tx_graph_changeset = self.graph.apply_update(update.graph_update);
        let chain_changeset = self.chain.apply_update(update.chain_update)?;
        self.db.lock().unwrap().execute(|tx| {
            let chain_list = self.chain_list_handle.api(&tx);
            let changed = !chain_changeset.is_empty() || !indexed_tx_graph_changeset.is_empty();

            if !chain_changeset.is_empty() {
                chain_list.push(&bincode::serde::Compat(chain_changeset))?;
            }

            self.consume_indexed_tx_graph_changeset(tx, indexed_tx_graph_changeset)?;

            Ok(changed)
        })
    }

    fn consume_indexed_tx_graph_changeset(
        &self,
        tx: &mut llsdb::Transaction<impl llsdb::Backend>,
        mut changeset: WalletIndexedTxGraphChangeSet,
    ) -> Result<()> {
        // We never want to add keychain descriptors to the database. We derive them from the keys.
        changeset.indexer.keychains_added.clear();
        if !changeset.graph.is_empty() {
            let tx_graph_list = self.tx_graph_list_handle.api(&tx);
            tx_graph_list.push(&bincode::serde::Compat(changeset.graph))?;
        }

        self.consume_spk_revelation_change(tx, changeset.indexer)?;

        Ok(())
    }

    pub fn send_to(
        &mut self,
        key_id: KeyId,
        to_address: bitcoin::Address,
        value: u64,
        feerate: f32,
    ) -> Result<BitcoinTransactionSignTask> {
        use bdk_coin_select::{
            metrics, Candidate, ChangePolicy, CoinSelector, DrainWeights, FeeRate, Target,
            TargetFee, TargetOutputs, TR_DUST_RELAY_MIN_VALUE, TR_KEYSPEND_TXIN_WEIGHT,
        };

        let utxos: Vec<(_, bdk_chain::FullTxOut<_>)> = self
            .graph
            .graph()
            .filter_chain_unspents(
                &self.chain,
                self.chain.tip().block_id(),
                self.graph
                    .index
                    .keychain_outpoints_in_range(Self::key_index_range(key_id)),
            )
            .collect();

        let candidates = utxos
            .iter()
            .map(|(_path, utxo)| Candidate {
                input_count: 1,
                value: utxo.txout.value.to_sat(),
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                is_segwit: true,
            })
            .collect::<Vec<_>>();

        let target_output = bitcoin::TxOut {
            script_pubkey: to_address.script_pubkey(),
            value: Amount::from_sat(value),
        };
        let mut outputs = vec![target_output];

        let target = Target {
            fee: TargetFee::from_feerate(FeeRate::from_sat_per_vb(feerate)),
            outputs: TargetOutputs::fund_outputs(
                outputs
                    .iter()
                    .map(|output| (output.weight().to_wu() as u32, output.value.to_sat())),
            ),
        };

        // we try and guess the usual feerate from the existing transactions in the graph This is
        // not a great heuristic since it doesn't focus on transactions the user has sent recently.
        let long_term_feerate_guess = {
            let feerates = self
                .graph
                .graph()
                .full_txs()
                .filter_map(|tx| {
                    Some(
                        self.graph.graph().calculate_fee(&tx).ok()?.to_sat() as f32
                            / tx.weight().to_wu() as f32,
                    )
                })
                .collect::<Vec<_>>();

            let mut average = feerates.iter().sum::<f32>() / feerates.len() as f32;

            if !average.is_normal() {
                average = 10.0;
            }
            FeeRate::from_sat_per_vb(average)
        };

        let drain_weights = DrainWeights::TR_KEYSPEND;
        let change_policy = ChangePolicy::min_value_and_waste(
            drain_weights,
            TR_DUST_RELAY_MIN_VALUE,
            target.fee.rate,
            long_term_feerate_guess,
        );

        let mut cs = CoinSelector::new(&candidates);
        let metric = metrics::LowestFee {
            target,
            long_term_feerate: long_term_feerate_guess,
            change_policy,
        };

        match cs.run_bnb(metric, 500_000) {
            Err(_) => {
                event!(Level::ERROR, "unable to find a slection with lowest fee");
                cs.select_until_target_met(target)?;
            }
            Ok(score) => {
                event!(Level::INFO, "coin selection succeeded with {score}");
            }
        }

        let selected_utxos = cs.apply_selection(&utxos);

        let mut inputs: Vec<bitcoin::TxIn> = vec![];
        let mut prevouts = vec![];

        for (((_key_id, keychain), i), selected_utxo) in selected_utxos {
            prevouts.push(frostsnap_core::message::TxInput {
                prevout: selected_utxo.txout.clone(),
                bip32_path: Some(vec![*keychain as _, *i]),
            });
            inputs.push(bitcoin::TxIn {
                previous_output: selected_utxo.outpoint,
                witness: Default::default(),
                script_sig: Default::default(),
                sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
            });
        }

        if let Some(value) = cs.drain_value(target, change_policy) {
            let ((i, change_spk), changeset) = self
                .graph
                .index
                .next_unused_spk(&(key_id, Keychain::Internal))
                .expect("this should have been initialzed by now since we are spending from it");
            let change_spk = change_spk.to_owned();
            self.db
                .lock()
                .unwrap()
                .execute(|tx| {
                    self.consume_spk_revelation_change(tx, changeset)?;
                    Ok(())
                })
                .context("trying to persist change derivation increment")?;
            self.graph.index.mark_used((key_id, Keychain::Internal), i);
            outputs.push(bitcoin::TxOut {
                script_pubkey: change_spk.to_owned(),
                value: Amount::from_sat(value),
            });
        }

        let tx_template = Transaction {
            version: bitcoin::transaction::Version(0x02),
            lock_time: bitcoin::absolute::LockTime::Blocks(
                bitcoin::absolute::Height::from_consensus(self.chain.tip().height())?,
            ),
            input: inputs,
            output: outputs,
        };

        Ok(BitcoinTransactionSignTask {
            tx_template,
            prevouts,
        })
    }

    pub fn complete_tx_sign_task(
        &self,
        task: BitcoinTransactionSignTask,
        signatures: Vec<EncodedSignature>,
    ) -> Result<Transaction> {
        let mut tx = task.tx_template;
        for (txin, signature) in tx.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                sig: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0).unwrap(),
                hash_ty: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.witness = witness;
        }
        Ok(tx)
    }

    pub fn spends_outside(&self, tx: &Transaction) -> BTreeMap<ScriptBuf, u64> {
        let mut foreign_outputs = BTreeMap::new();

        for txout in &tx.output {
            let is_owned_by_our_wallet = self
                .graph
                .index
                .index_of_spk(&txout.script_pubkey)
                .is_some();
            if !is_owned_by_our_wallet {
                let value = foreign_outputs
                    .entry(txout.script_pubkey.clone())
                    .or_default();
                *value += txout.value.to_sat();
            }
        }
        foreign_outputs
    }

    pub fn net_value(&self, key_id: KeyId, tx: &Transaction) -> i64 {
        self.graph
            .index
            .net_value(tx, Self::key_index_range(key_id))
            .to_sat()
    }

    fn key_index_range(key_id: KeyId) -> impl RangeBounds<(KeyId, Keychain)> {
        (key_id, Keychain::External)..=(key_id, Keychain::Internal)
    }

    pub fn fee(&self, tx: &Transaction) -> Result<u64> {
        let fee = self.graph.graph().calculate_fee(tx)?;
        Ok(fee.to_sat())
    }

    pub fn broadcast_success(&mut self, tx: Transaction) {
        let mut changes = WalletIndexedTxGraphChangeSet::default();
        changes.append(
            self.graph.insert_seen_at(
                tx.txid(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        );
        changes.append(self.graph.insert_tx(tx));

        // We do our best here, if it fails to persist we should recover from this eventually
        let res = self.db.lock().unwrap().execute(|tx| {
            self.consume_indexed_tx_graph_changeset(tx, changes)?;
            Ok(())
        });

        if let Err(e) = res {
            event!(
                Level::ERROR,
                error = e.to_string(),
                "failed to persist broadcast"
            );
        }
    }

    pub fn xpub(&self, key_id: KeyId) -> Result<Option<bip32::Xpub>> {
        Ok(self.get_frost_key(key_id)?.map(|coord_frost_key| {
            coord_frost_key
                .frost_key()
                .bitcoin_app_xpub()
                .xpub(self.network)
                .clone()
        }))
    }

    pub fn psbt_to_unsigned_tx(
        &self,
        psbt: &bitcoin::psbt::Psbt,
        key_id: KeyId,
    ) -> Result<UnsignedTx> {
        let frost_key_xpub = self.xpub(key_id)?.ok_or(anyhow!("no such key {key_id}"))?;
        let our_fingerprint = frost_key_xpub.fingerprint();
        let mut prevouts = vec![];
        let secp = secp256k1::Secp256k1::verification_only();

        for (i, input) in psbt.inputs.iter().enumerate() {
            let txout = match &input.witness_utxo {
                Some(txout) => txout,
                None => {
                    event!(
                        Level::INFO,
                        "Skipping signing PSBT input {i} because it doesn't have a witness_utxo"
                    );
                    continue;
                }
            };

            prevouts.push(frostsnap_core::message::TxInput {
                prevout: txout.clone(),
                bip32_path: None,
            });
            let prevout = prevouts.last_mut().unwrap();

            let tap_internal_key = match &input.tap_internal_key {
                Some(tap_internal_key) => tap_internal_key,
                None => {
                    event!(Level::INFO,
                        "Skipping signing PSBT input {i} because it doesn't have an tap_internal_key"
                    );
                    continue;
                }
            };

            let (fingerprint, derivation_path) = match input.tap_key_origins.get(tap_internal_key) {
                Some(origin) => origin.1.clone(),
                None => {
                    event!(Level::INFO,"Skipping signing PSBT input {i} because it doesn't provide a source for the tap_internal_key");
                    continue;
                }
            };

            if fingerprint != our_fingerprint {
                event!(Level::INFO,"Skipping signing PSBT input {i} because internal key fingerprint doesn't match ours");
                continue;
            }

            let input_xpub = frost_key_xpub.derive_pub(&secp, &derivation_path).unwrap();

            if input_xpub.to_x_only_pub() != *tap_internal_key {
                return Err(anyhow!("Corrupt PSBT -- The key's fingerprint matches but the derived key does not match the tap_internal_key"))?;
            }

            prevout.bip32_path = Some(derivation_path.to_u32_vec());
        }

        Ok(UnsignedTx {
            task: RustOpaque::new(frostsnap_core::message::BitcoinTransactionSignTask {
                tx_template: psbt.clone().extract_tx()?,
                prevouts,
            }),
        })
    }

    pub fn add_signatures_to_psbt(
        &self,
        psbt: &bitcoin::psbt::Psbt,
        signatures: Vec<EncodedSignature>,
    ) -> Result<bitcoin::psbt::Psbt> {
        let mut psbt = psbt.clone();
        for (txin, signature) in psbt.inputs.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                sig: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0).unwrap(),
                hash_ty: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.final_script_witness = Some(witness);
        }

        Ok(psbt.clone())
    }
}
