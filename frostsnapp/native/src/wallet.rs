use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self, ScriptBuf, Transaction},
    indexed_tx_graph::{self, IndexedTxGraph},
    keychain::KeychainTxOutIndex,
    local_chain::{self, LocalChain},
    miniscript::{
        descriptor::{DescriptorXKey, Tr, Wildcard},
        Descriptor, DescriptorPublicKey,
    },
    tx_graph::{self},
    Append, ChainPosition, ConfirmationTimeHeightAnchor, FullTxOut,
};
use flutter_rust_bridge::RustOpaque;
use frostsnap_coordinator::frostsnap_core::{
    self,
    message::{EncodedSignature, TransactionSignTask},
    schnorr_fun::{frost::FrostKey, fun::marker::Normal},
    FrostKeyExt, KeyId,
};
use llsdb::{IndexHandle, LinkedList, LlsDb};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};

use crate::{
    api,
    chain_sync::{self, SyncRequest},
    persist_core::PersistCore,
};

// Flutter rust bridge is annoyed if I call this `Wallet`
pub struct _Wallet {
    graph: IndexedTxGraph<ConfirmationTimeHeightAnchor, KeychainTxOutIndex<KeyId>>,
    chain: LocalChain,
    pub network: bitcoin::Network,
    db: Arc<Mutex<LlsDb<File>>>,
    chain_list_handle: LinkedList<bincode::serde::Compat<ChainChangeSet>>,
    tx_graph_list_handle: LinkedList<bincode::serde::Compat<TxGraphChangeSet>>,
    keychain_handle: IndexHandle<llsdb::index::BTreeMap<KeyId, u32>>,
    persist_core: IndexHandle<PersistCore>,
}

pub type TxGraphChangeSet = tx_graph::ChangeSet<ConfirmationTimeHeightAnchor>;
pub type ChainChangeSet = local_chain::ChangeSet;

impl _Wallet {
    pub fn load_or_init(
        db: Arc<Mutex<LlsDb<File>>>,
        network: bitcoin::Network,
        persist_core: IndexHandle<PersistCore>,
    ) -> anyhow::Result<Self> {
        event!(Level::INFO, "initializing wallet");
        let mut db_ = db.lock().unwrap();
        let (chain, graph, network, tx_graph_list_handle, chain_list_handle, keychain_handle) = db_
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

                let keychain_list_handle = tx
                    .take_list::<(KeyId, u32)>("wallet/keychain")
                    .context("loading keychain list")?;
                let keychain_index = llsdb::index::BTreeMap::new(keychain_list_handle, &tx)
                    .context("indexing keychain list")?;
                let chain_list = chain_list_handle.api(&tx);
                let tx_list = tx_graph_list_handle.api(&tx);
                let keychain_handle = tx.store_index(keychain_index);
                let keychain = tx.take_index(keychain_handle);
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

                    let mut graph = IndexedTxGraph::<
                        ConfirmationTimeHeightAnchor,
                        KeychainTxOutIndex<KeyId>,
                    >::default();

                    let persisted_frost_keys = tx
                        .take_index(persist_core)
                        .coord_frost_keys()
                        .context("reading persisted frost keys")?;
                    for coord_frost_key in persisted_frost_keys {
                        let key_id = coord_frost_key.key_id();
                        graph.index.add_keychain(
                            key_id,
                            Self::get_descriptor(coord_frost_key.frost_key()),
                        );
                        if let Some(derivation_index) = keychain.get(&key_id)? {
                            let _ = graph.index.reveal_to_target(&key_id, derivation_index);
                        }
                    }

                    graph.apply_changeset(tx_data.into());
                    let chain = LocalChain::from_changeset(chain_data)?;
                    (chain, graph)
                };
                Ok((
                    chain,
                    graph,
                    network,
                    tx_graph_list_handle,
                    chain_list_handle,
                    keychain_handle,
                ))
            })
            .context("Initializing wallet from ")?;

        event!(Level::INFO, "wallet initialization finished");

        drop(db_);
        Ok(Self {
            db,
            chain,
            graph,
            network,
            tx_graph_list_handle,
            chain_list_handle,
            keychain_handle,
            persist_core,
        })
    }

    fn get_descriptor(frost_key: &FrostKey<Normal>) -> Descriptor<DescriptorPublicKey> {
        use bitcoin::bip32::DerivationPath;
        let frost_xpub = frostsnap_core::xpub::Xpub::new(frost_key.clone());
        let key = DescriptorPublicKey::XPub(DescriptorXKey {
            origin: None,
            xkey: *frost_xpub.xpub(),
            derivation_path: DerivationPath::master(),
            wildcard: Wildcard::Unhardened,
        });

        let tr = Tr::new(key, None).expect("infallible since it's None");
        Descriptor::Tr(tr)
    }

    fn lazily_initialize_key(&mut self, key_id: KeyId) -> Result<()> {
        if !self.graph.index.keychains().contains_key(&key_id) {
            let coord_frost_keys = self
                .db
                .lock()
                .unwrap()
                .execute(|tx| tx.take_index(self.persist_core).coord_frost_keys())?;
            let found = coord_frost_keys
                .into_iter()
                .find(|coord_frost_key| coord_frost_key.frost_key().key_id() == key_id)
                .ok_or(anyhow!("key {key_id} doesn't exist in database"))?;
            self.graph
                .index
                .add_keychain(key_id, Self::get_descriptor(found.frost_key()));
        }
        Ok(())
    }

    pub fn list_addresses(&self, key_id: KeyId) -> Vec<api::Address> {
        self.graph
            .index
            .revealed_keychain_spks(&key_id)
            .rev()
            .map(|(i, spk)| api::Address {
                index: i,
                address_string: bitcoin::Address::from_script(spk, self.network)
                    .expect("has address form")
                    .to_string(),
                used: self.graph.index.is_used(key_id, i),
            })
            .collect()
    }

    pub fn next_address(&mut self, key_id: KeyId) -> Result<api::Address> {
        // We don't know in the wallet when a new key has been created so we need to lazily
        // initialze the wallet with it when we first get asked for an address.
        self.lazily_initialize_key(key_id)?;
        let ((index, spk), changeset) = self.graph.index.reveal_next_spk(&key_id);
        let spk = spk.to_owned();
        self.db
            .lock()
            .unwrap()
            .execute(|tx| tx.take_index(self.keychain_handle).extend(changeset.0))?;
        // TODO: There should be a way of unrevealing index if we fail to persist:
        // https://github.com/bitcoindevkit/bdk/issues/1322
        Ok(api::Address {
            index,
            address_string: bitcoin::Address::from_script(&spk, self.network)
                .expect("has address form")
                .to_string(),
            used: self.graph.index.is_used(key_id, index),
        })
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
                    .net_value(canonical_tx.tx_node.tx, key_id..=key_id);

                if net_value == 0 {
                    return None;
                }
                Some(api::Transaction {
                    inner: RustOpaque::new(canonical_tx.tx_node.tx.clone()),
                    confirmation_time,
                    net_value,
                })
            })
            .collect()
    }

    pub fn sync_txs(&self, txids: Vec<bitcoin::Txid>) -> SyncRequest {
        let mut sync = SyncRequest::new(self.chain.tip(), self.graph.graph().clone());
        sync.add_txids(txids);
        sync
    }

    pub fn start_sync(&self, key_id: KeyId) -> SyncRequest {
        // We want to sync all spks for now!
        let interesting_spks = self
            .graph
            .index
            .revealed_spks(key_id..=key_id)
            .map(|(_, _, spk)| spk.to_owned())
            .collect::<Vec<_>>();

        let mut sync = SyncRequest::new(self.chain.tip(), self.graph.graph().clone());
        sync.add_spks(interesting_spks);
        sync
    }

    pub fn finish_sync(&mut self, update: chain_sync::Update) -> Result<bool> {
        self.db.lock().unwrap().execute(|tx| {
            let chain_list = self.chain_list_handle.api(&tx);
            let tx_graph_list = self.tx_graph_list_handle.api(&tx);
            let chain_changeset = self.chain.apply_update(update.chain)?;
            if !chain_changeset.is_empty() {
                chain_list.push(&bincode::serde::Compat(chain_changeset))?;
            }
            let graph_changeset = self.graph.apply_update(update.tx_graph);
            // See bug: https://github.com/bitcoindevkit/bdk/pull/1335 for why this disjunction is needed
            if !(graph_changeset.is_empty() && graph_changeset.graph.anchors.is_empty()) {
                tx_graph_list.push(&bincode::serde::Compat(graph_changeset.graph))?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    pub fn send_to(
        &mut self,
        key_id: KeyId,
        to_address: bitcoin::Address,
        value: u64,
        feerate: f32,
    ) -> Result<TransactionSignTask> {
        use bdk_coin_select::{
            metrics, Candidate, ChangePolicy, CoinSelector, DrainWeights, FeeRate, Target,
            TargetFee, TargetOutputs, TR_DUST_RELAY_MIN_VALUE, TR_KEYSPEND_TXIN_WEIGHT,
        };

        let utxos: Vec<((KeyId, u32), FullTxOut<_>)> = self
            .graph
            .graph()
            .filter_chain_unspents(
                &self.chain,
                self.chain.tip().block_id(),
                self.graph.index.outpoints().iter().cloned(),
            )
            .collect();

        let candidates = utxos
            .iter()
            .map(|(_path, utxo)| Candidate {
                input_count: 1,
                value: utxo.txout.value,
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                is_segwit: true,
            })
            .collect::<Vec<_>>();

        let target_output = bitcoin::TxOut {
            script_pubkey: to_address.script_pubkey(),
            value,
        };
        let mut outputs = vec![target_output];

        let target = Target {
            fee: TargetFee::from_feerate(FeeRate::from_sat_per_vb(feerate)),
            outputs: TargetOutputs::fund_outputs(
                outputs
                    .iter()
                    .map(|output| (output.weight() as u32, output.value)),
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
                        self.graph.graph().calculate_fee(&tx).ok()? as f32
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

        for ((_, i), selected_utxo) in selected_utxos {
            prevouts.push(frostsnap_core::message::TxInput {
                prevout: selected_utxo.txout.clone(),
                bip32_path: Some(vec![*i]),
            });
            inputs.push(bitcoin::TxIn {
                previous_output: selected_utxo.outpoint,
                witness: Default::default(),
                script_sig: Default::default(),
                sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
            });
        }

        if let Some(value) = cs.drain_value(target, change_policy) {
            let ((i, change_spk), changeset) = self.graph.index.next_unused_spk(&key_id);
            let change_spk = change_spk.to_owned();
            self.db
                .lock()
                .unwrap()
                .execute(|tx| tx.take_index(self.keychain_handle).extend(changeset.0))
                .context("trying to persist change derivation increment")?;
            self.graph.index.mark_used(key_id, i);
            outputs.push(bitcoin::TxOut {
                script_pubkey: change_spk.to_owned(),
                value,
            });
        }

        let tx_template = Transaction {
            version: 0x02,
            lock_time: bitcoin::absolute::LockTime::Blocks(
                bitcoin::absolute::Height::from_consensus(self.chain.tip().height())?,
            ),
            input: inputs,
            output: outputs,
        };

        Ok(TransactionSignTask {
            tx_template,
            prevouts,
        })
    }

    pub fn complete_tx_sign_task(
        &self,
        task: TransactionSignTask,
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
        let mut outputs = BTreeMap::new();

        for txout in &tx.output {
            if self
                .graph
                .index
                .index_of_spk(&txout.script_pubkey)
                .is_none()
            {
                outputs
                    .entry(txout.script_pubkey.clone())
                    .and_modify(|v| *v += txout.value)
                    .or_insert(txout.value);
            }
        }

        outputs
    }

    pub fn net_value(&self, key_id: KeyId, tx: &Transaction) -> i64 {
        self.graph.index.net_value(tx, key_id..=key_id)
    }

    pub fn fee(&self, tx: &Transaction) -> Result<u64> {
        let fee = self.graph.graph().calculate_fee(tx)?;
        Ok(fee)
    }

    pub fn broadcast_success(&mut self, tx: Transaction) {
        let mut changes = indexed_tx_graph::ChangeSet::default();
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

        // we do our best here, if it fails to persist we should recover from this eventually
        let res = self.db.lock().unwrap().execute(|tx| {
            if !changes.graph.is_empty() {
                self.tx_graph_list_handle
                    .api(&tx)
                    .push(&bincode::serde::Compat(changes.graph))?;
            }
            tx.take_index(self.keychain_handle)
                .extend(changes.indexer.0)?;
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
}
