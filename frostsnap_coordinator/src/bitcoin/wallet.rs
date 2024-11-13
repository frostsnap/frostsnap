use super::chain_sync::SyncRequest;
use crate::persist::Persisted;
use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self, bip32, Amount, SignedAmount},
    indexed_tx_graph,
    indexer::keychain_txout::{self, KeychainTxOutIndex},
    local_chain,
    miniscript::{Descriptor, DescriptorPublicKey},
    spk_client::{self, SyncRequestBuilder},
    ChainPosition, ConfirmationBlockTime, Indexer, Merge,
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
use std::{
    ops::RangeBounds,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};

pub type WalletIndexedTxGraph = indexed_tx_graph::IndexedTxGraph<
    ConfirmationBlockTime,
    KeychainTxOutIndex<(KeyId, AppAccountKeychain)>,
>;
pub type WalletIndexedTxGraphChangeSet =
    indexed_tx_graph::ChangeSet<ConfirmationBlockTime, keychain_txout::ChangeSet>;

/// Pretty much a generic bitcoin wallet that indexes everything by key id
pub struct FrostsnapWallet {
    tx_graph: Persisted<WalletIndexedTxGraph>,
    chain: Persisted<local_chain::LocalChain>,
    pub network: bitcoin::Network,
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl FrostsnapWallet {
    pub fn load_or_init(
        db: Arc<Mutex<rusqlite::Connection>>,
        network: bitcoin::Network,
    ) -> anyhow::Result<Self> {
        event!(Level::INFO, "initializing wallet");
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
            db,
            network,
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
            .tx_graph
            .index
            .get_descriptor((key_id, AppAccountKeychain::external()))
            .is_none()
        {
            for (account_keychain, descriptor) in Self::descriptors_for_key(
                key_id.to_root_pubkey().expect("valid key id"),
                self.network,
            ) {
                let _intentionally_ignore_saving_descriptors = self
                    .tx_graph
                    .MUTATE_NO_PERSIST()
                    .index
                    .insert_descriptor((key_id, account_keychain), descriptor);
            }
            let all_txs = self
                .tx_graph
                .graph()
                .full_txs()
                .map(|tx| tx.tx.clone())
                .collect::<Vec<_>>();
            // FIXME: This should be done by BDK automatically in a version soon
            for tx in &all_txs {
                let _ = self.tx_graph.MUTATE_NO_PERSIST().index.index_tx(tx);
            }
        }
    }

    pub fn list_addresses(&mut self, key_id: KeyId) -> Vec<AddressInfo> {
        self.lazily_initialize_key(key_id);
        self.tx_graph
            .index
            .revealed_keychain_spks((key_id, AppAccountKeychain::external()))
            .rev()
            .map(|(i, spk)| AddressInfo {
                index: i,
                address: bitcoin::Address::from_script(&spk, self.network)
                    .expect("has address form"),
                used: self
                    .tx_graph
                    .index
                    .is_used((key_id, AppAccountKeychain::external()), i),
            })
            .collect()
    }

    pub fn next_address(&mut self, key_id: KeyId) -> Result<AddressInfo> {
        self.lazily_initialize_key(key_id);

        let mut db = self.db.lock().unwrap();
        let (index, spk) = self.tx_graph.mutate(&mut *db, |tx_graph| {
            tx_graph
                .index
                .reveal_next_spk((key_id, AppAccountKeychain::external()))
                .ok_or(anyhow!("no more addresses on this keychain"))
        })?;

        Ok(AddressInfo {
            index,
            address: bitcoin::Address::from_script(&spk, self.network).expect("has address form"),
            used: self
                .tx_graph
                .index
                .is_used((key_id, AppAccountKeychain::external()), index),
        })
    }

    pub fn list_transactions(&mut self, key_id: KeyId) -> Vec<Transaction> {
        self.lazily_initialize_key(key_id);
        let mut txs = self
            .tx_graph
            .graph()
            .list_canonical_txs(self.chain.as_ref(), self.chain.tip().block_id())
            .collect::<Vec<_>>();

        txs.sort_unstable_by_key(|tx| core::cmp::Reverse(tx.chain_position));
        txs.into_iter()
            .filter_map(|canonical_tx| {
                let confirmation_time = match canonical_tx.chain_position {
                    ChainPosition::Confirmed(conf_time) => Some(ConfirmationTime {
                        height: conf_time.block_id.height,
                        time: conf_time.confirmation_time,
                    }),
                    _ => None,
                };
                let net_value = self
                    .tx_graph
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

    pub fn sync_txs(&self, txids: impl IntoIterator<Item = bitcoin::Txid>) -> SyncRequestBuilder {
        // TODO: For consistency, rename this to `start_sync_with_txs`?
        SyncRequest::builder()
            .chain_tip(self.chain.tip())
            .txids(txids)
    }

    pub fn start_sync(&self, key_id: KeyId) -> SyncRequestBuilder {
        // We want to sync all spks for now!
        let interesting_spks = self
            .tx_graph
            .index
            .revealed_spks(Self::key_index_range(key_id))
            .map(|(_, spk)| spk.to_owned());
        SyncRequest::builder()
            .chain_tip(self.chain.tip())
            .spks(interesting_spks)
    }

    pub fn finish_sync(
        &mut self,
        update: spk_client::SyncResult<ConfirmationBlockTime>,
    ) -> Result<bool> {
        let mut db = self.db.lock().unwrap();

        let changed =
            self.tx_graph
                .multi(&mut self.chain)
                .mutate(&mut *db, |tx_graph, chain| {
                    let changeset_tx = tx_graph.apply_update(update.tx_update);
                    let changeset_chain = match update.chain_update {
                        Some(chain_update) => chain.apply_update(chain_update)?,
                        None => local_chain::ChangeSet::default(),
                    };
                    let changed = !(changeset_tx.is_empty() && changeset_chain.is_empty());
                    Ok((changed, (changeset_tx, changeset_chain)))
                })?;

        Ok(changed)
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
            .tx_graph
            .graph()
            .filter_chain_unspents(
                self.chain.as_ref(),
                self.chain.tip().block_id(),
                self.tx_graph
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
                .tx_graph
                .graph()
                .full_txs()
                .filter_map(|tx| {
                    Some(
                        self.tx_graph.graph().calculate_fee(&tx).ok()?.to_sat() as f32
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
                .tx_graph
                .graph()
                .get_tx(selected_utxo.outpoint.txid)
                .expect("must exist");
            let bip32_path = AppBip32Path {
                account_keychain: *account_keychain,
                index: *index,
            };
            template_tx.push_owned_input(
                PushInput::spend_tx_output(prev_tx.as_ref(), selected_utxo.outpoint.vout),
                LocalSpk {
                    root_key,
                    bip32_path,
                },
            )?;
        }

        if let Some(value) = cs.drain_value(target, change_policy) {
            let mut db = self.db.lock().unwrap();
            let (i, _change_spk) = self.tx_graph.mutate(&mut *db, |tx_graph| {
                Ok(tx_graph
                    .index
                    .next_unused_spk((key_id, AppAccountKeychain::internal()))
                    .expect(
                        "this should have been initialzed by now since we are spending from it",
                    ))
            })?;

            self.tx_graph
                .MUTATE_NO_PERSIST()
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
        let fee = self.tx_graph.graph().calculate_fee(tx)?;
        Ok(fee.to_sat())
    }

    pub fn broadcast_success(&mut self, tx: bitcoin::Transaction) {
        // We do our best here, if it fails to persist we should recover from this eventually
        let res = self
            .tx_graph
            .mutate(&mut *self.db.lock().unwrap(), |tx_graph| {
                let mut changeset = WalletIndexedTxGraphChangeSet::default();
                changeset.merge(
                    tx_graph.insert_seen_at(
                        tx.compute_txid(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    ),
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
            match self
                .tx_graph
                .index
                .index_of_spk(txout.script_pubkey.clone())
            {
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
