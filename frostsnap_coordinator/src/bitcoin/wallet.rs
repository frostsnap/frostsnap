use super::chain_sync::SyncRequest;
use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self, bip32, Amount, SignedAmount},
    indexed_tx_graph::{self, Indexer},
    keychain::{self, KeychainTxOutIndex},
    local_chain::{self, LocalChain},
    miniscript::{Descriptor, DescriptorPublicKey},
    spk_client, tx_graph, Append, ChainPosition, ConfirmationTimeHeightAnchor,
};
use frostsnap_core::{
    bitcoin_transaction::{self, LocalSpk},
    tweak::{Account, AppAccountKeychain, AppBip32Path, Keychain},
    KeyId,
};
use frostsnap_core::{
    bitcoin_transaction::{PushInput, TransactionTemplate},
    schnorr_fun::fun::Point,
    tweak::TweakableKey,
};
use llsdb::{LinkedList, LlsDb};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    ops::RangeBounds,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};

type TxGraphChangeSet = tx_graph::ChangeSet<ConfirmationTimeHeightAnchor>;
type ChainChangeSet = local_chain::ChangeSet;
type WalletIndexedTxGraph = indexed_tx_graph::IndexedTxGraph<
    ConfirmationTimeHeightAnchor,
    KeychainTxOutIndex<(KeyId, AppAccountKeychain)>,
>;
type WalletIndexedTxGraphChangeSet = indexed_tx_graph::ChangeSet<
    ConfirmationTimeHeightAnchor,
    keychain::ChangeSet<(KeyId, AppAccountKeychain)>,
>;

/// Pretty much a generic bitcoin wallet that indexes everything by key id
pub struct FrostsnapWallet {
    graph: WalletIndexedTxGraph,
    chain: LocalChain,
    pub network: bitcoin::Network,
    db: Arc<Mutex<LlsDb<File>>>,
    chain_list_handle: LinkedList<bincode::serde::Compat<ChainChangeSet>>,
    tx_graph_list_handle: LinkedList<bincode::serde::Compat<TxGraphChangeSet>>,
    /// Which spks have been revealed for which descriptors
    spk_revelation_list_handle:
        LinkedList<bincode::serde::Compat<BTreeMap<bdk_chain::DescriptorId, u32>>>,
}

impl FrostsnapWallet {
    pub fn load_or_init(
        db: Arc<Mutex<LlsDb<File>>>,
        network: bitcoin::Network,
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
                    .take_list::<bincode::serde::Compat<ChainChangeSet>>("wallet/chain")
                    .context("loading chain list")?;
                let tx_graph_list_handle = tx
                    .take_list::<bincode::serde::Compat<TxGraphChangeSet>>("wallet/tx_graph")
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
                        let mut full_changeset = ChainChangeSet::default();
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
        })
    }

    fn descriptors_for_key(
        root_key: Point,
        network: bitcoin::Network,
    ) -> Vec<(AppAccountKeychain, Descriptor<DescriptorPublicKey>)> {
        [
            AppAccountKeychain::external(),
            AppAccountKeychain::internal(),
        ]
        .into_iter()
        .zip(
            //XXX: this logic is very brittle and implicit with respect to accounts
            super::multi_x_descriptor_for_account(root_key, Account::Segwitv1, network)
                .into_single_descriptors()
                .expect("should be well formed"),
        )
        .collect()
    }

    fn lazily_initialize_key(&mut self, key_id: KeyId) {
        if self
            .graph
            .index
            .get_descriptor(&(key_id, AppAccountKeychain::external()))
            .is_none()
        {
            for (account_keychain, descriptor) in Self::descriptors_for_key(
                key_id.to_root_pubkey().expect("valid key id"),
                self.network,
            ) {
                let _intentionally_ignore_saving_descriptors = self
                    .graph
                    .index
                    .insert_descriptor((key_id, account_keychain), descriptor);
            }
            let all_txs = self
                .graph
                .graph()
                .full_txs()
                .map(|tx| tx.tx.clone())
                .collect::<Vec<_>>();
            // FIXME: This should be done by BDK automatically in a version soon
            for tx in &all_txs {
                let _ = self.graph.index.index_tx(tx);
            }
        }
    }

    pub fn list_addresses(&mut self, key_id: KeyId) -> Vec<AddressInfo> {
        self.lazily_initialize_key(key_id);
        self.graph
            .index
            .revealed_keychain_spks(&(key_id, AppAccountKeychain::external()))
            .rev()
            .map(|(i, spk)| AddressInfo {
                index: i,
                address: bitcoin::Address::from_script(spk, self.network)
                    .expect("has address form"),
                used: self
                    .graph
                    .index
                    .is_used((key_id, AppAccountKeychain::external()), i),
            })
            .collect()
    }

    pub fn next_address(&mut self, key_id: KeyId) -> Result<AddressInfo> {
        self.lazily_initialize_key(key_id);

        if let Some(((index, spk), changeset)) = self
            .graph
            .index
            .reveal_next_spk(&(key_id, AppAccountKeychain::external()))
        {
            let spk = spk.to_owned();
            self.db.lock().unwrap().execute(|tx| {
                self.consume_spk_revelation_change(tx, changeset)?;
                Ok(())
            })?;
            // TODO: There should be a way of unrevealing index if we fail to persist:
            // https://github.com/bitcoindevkit/bdk/issues/1322
            Ok(AddressInfo {
                index,
                address: bitcoin::Address::from_script(&spk, self.network)
                    .expect("has address form"),
                used: self
                    .graph
                    .index
                    .is_used((key_id, AppAccountKeychain::external()), index),
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

    pub fn list_transactions(&mut self, key_id: KeyId) -> Vec<Transaction> {
        self.lazily_initialize_key(key_id);
        let mut txs = self
            .graph
            .graph()
            .list_canonical_txs(&self.chain, self.chain.tip().block_id())
            .collect::<Vec<_>>();

        txs.sort_unstable_by_key(|tx| core::cmp::Reverse(tx.chain_position));
        txs.into_iter()
            .filter_map(|canonical_tx| {
                let confirmation_time = match canonical_tx.chain_position {
                    ChainPosition::Confirmed(conf_time) => Some(ConfirmationTime {
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
                Some(Transaction {
                    inner: canonical_tx.tx_node.tx.clone(),
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
    ) -> Result<bitcoin_transaction::TransactionTemplate> {
        self.lazily_initialize_key(key_id);
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

        let target = Target {
            fee: TargetFee::from_feerate(FeeRate::from_sat_per_vb(feerate)),
            outputs: TargetOutputs::fund_outputs(vec![(
                target_output.weight().to_wu() as u32,
                target_output.value.to_sat(),
            )]),
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

        let mut template_tx = frostsnap_core::bitcoin_transaction::TransactionTemplate::new();

        // let mut inputs: Vec<bitcoin::TxIn> = vec![];
        // let mut prevouts = vec![];
        let root_key = key_id.to_root_pubkey().ok_or(anyhow!("invalid key id"))?;

        for (((_key_id, account_keychain), index), selected_utxo) in selected_utxos {
            let prev_tx = self
                .graph
                .graph()
                .get_tx(selected_utxo.outpoint.txid)
                .expect("must exist");
            let bip32_path = AppBip32Path {
                account_keychain: *account_keychain,
                index: *index,
            };
            template_tx.push_owned_input(
                PushInput::spend_tx_output(&prev_tx, selected_utxo.outpoint.vout),
                LocalSpk {
                    root_key,
                    bip32_path,
                },
            )?;
        }

        if let Some(value) = cs.drain_value(target, change_policy) {
            let ((i, _change_spk), changeset) = self
                .graph
                .index
                .next_unused_spk(&(key_id, AppAccountKeychain::internal()))
                .expect("this should have been initialzed by now since we are spending from it");
            self.db
                .lock()
                .unwrap()
                .execute(|tx| {
                    self.consume_spk_revelation_change(tx, changeset)?;
                    Ok(())
                })
                .context("trying to persist change derivation increment")?;
            self.graph
                .index
                .mark_used((key_id, AppAccountKeychain::internal()), i);
            template_tx.push_owned_output(
                Amount::from_sat(value),
                LocalSpk {
                    root_key,
                    bip32_path: AppBip32Path {
                        account_keychain: AppAccountKeychain::internal(),
                        index: i,
                    },
                },
            );
        }

        template_tx.push_foreign_output(target_output);

        Ok(template_tx)
    }

    fn key_index_range(key_id: KeyId) -> impl RangeBounds<(KeyId, AppAccountKeychain)> {
        (
            key_id,
            AppAccountKeychain {
                account: Account::Segwitv1,
                keychain: Keychain::External,
            },
        )
            ..=(
                key_id,
                AppAccountKeychain {
                    account: Account::Segwitv1,
                    keychain: Keychain::Internal,
                },
            )
    }

    pub fn fee(&self, tx: &bitcoin::Transaction) -> Result<u64> {
        let fee = self.graph.graph().calculate_fee(tx)?;
        Ok(fee.to_sat())
    }

    pub fn broadcast_success(&mut self, tx: bitcoin::Transaction) {
        let mut changes = WalletIndexedTxGraphChangeSet::default();
        changes.append(
            self.graph.insert_seen_at(
                tx.compute_txid(),
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

    pub fn psbt_to_tx_template(
        &mut self,
        psbt: &bitcoin::Psbt,
        root_key: Point,
    ) -> Result<TransactionTemplate> {
        let xpub = root_key
            .bitcoin_app_xpub()
            .xpub(self.network /* note this is irrelevant */);
        let our_fingerprint = xpub.fingerprint();
        let mut template = frostsnap_core::bitcoin_transaction::TransactionTemplate::new();
        let rust_bitcoin_tx = &psbt.unsigned_tx;
        template.set_version(rust_bitcoin_tx.version);
        template.set_lock_time(rust_bitcoin_tx.lock_time);

        for (i, input) in psbt.inputs.iter().enumerate() {
            let txin = rust_bitcoin_tx
                .input
                .get(i)
                .ok_or(anyhow!("PSBT input {i} is malformed"))?;

            let txout = input
                .witness_utxo
                .as_ref()
                .or_else(|| {
                    let tx = input.non_witness_utxo.as_ref()?;
                    tx.output.get(txin.previous_output.vout as usize)
                })
                .ok_or(anyhow!(
                    "PSBT input {i} missing witness and non-witness utxo"
                ))?;

            let input_push =
                PushInput::spend_outpoint(txout, txin.previous_output).with_sequence(txin.sequence);

            macro_rules! bail {
                ($($reason:tt)*) => {{
                    event!(
                        Level::INFO,
                        "Skipping signing PSBT input {i} because it {}", $($reason)*
                    );
                    template.push_foreign_input(input_push);
                    continue;

                }};
            }

            if input.final_script_witness.is_some() {
                bail!("it already has a final_script_witness");
            }

            let tap_internal_key = match &input.tap_internal_key {
                Some(tap_internal_key) => tap_internal_key,
                None => bail!("it doesn't have an tap_internal_key"),
            };

            let (fingerprint, derivation_path) = match input.tap_key_origins.get(tap_internal_key) {
                Some(origin) => origin.1.clone(),
                None => bail!("it doesn't provide a source for the tap_internal_key"),
            };

            if fingerprint != our_fingerprint {
                bail!("it's key fingerprint doesn't match our root key");
            }

            let normal_derivation_path = derivation_path
                .into_iter()
                .map(|child_number| match child_number {
                    bip32::ChildNumber::Normal { index } => Ok(*index),
                    _ => Err(anyhow!("can't sign with hardended derivation")),
                })
                .collect::<Result<Vec<_>>>()?;

            let bip32_path = match AppBip32Path::from_u32_slice(&normal_derivation_path) {
                Some(bip32_path) => bip32_path,
                None => {
                    bail!("it has an unusual derivation path");
                }
            };

            template.push_owned_input(
                input_push,
                frostsnap_core::bitcoin_transaction::LocalSpk {
                    root_key,
                    bip32_path,
                },
            )?;
        }

        for (i, _) in psbt.outputs.iter().enumerate() {
            let txout = &rust_bitcoin_tx.output[i];
            match self.graph.index.index_of_spk(&txout.script_pubkey) {
                Some(&((_, account_keychain), index)) => template.push_owned_output(
                    txout.value,
                    LocalSpk {
                        root_key,
                        bip32_path: AppBip32Path {
                            account_keychain,
                            index,
                        },
                    },
                ),
                None => {
                    template.push_foreign_output(txout.clone());
                }
            }
        }

        Ok(template)
    }
}

#[derive(Clone, Debug)]
pub struct AddressInfo {
    pub index: u32,
    pub address: bitcoin::Address,
    pub used: bool,
}

#[derive(Clone, Debug)]
pub struct Transaction {
    pub net_value: i64,
    pub inner: Arc<bitcoin::Transaction>,
    pub confirmation_time: Option<ConfirmationTime>,
}

#[derive(Clone, Debug)]
pub struct ConfirmationTime {
    pub height: u32,
    pub time: u64,
}
