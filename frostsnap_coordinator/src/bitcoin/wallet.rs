use super::{chain_sync::ChainClient, multi_x_descriptor_for_account};
use crate::persist::Persisted;
use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self, bip32, Amount, BlockHash, OutPoint, ScriptBuf, TxOut, Txid},
    indexed_tx_graph::{self},
    indexer::keychain_txout::{self, KeychainTxOutIndex},
    local_chain::{self, LocalChain},
    miniscript::{Descriptor, DescriptorPublicKey},
    CanonicalTx, CanonicalView, CanonicalizationParams, CheckPoint, ConfirmationBlockTime,
    FullTxOut, Indexer, Merge,
};
use bdk_core::KeychainIndexed;
use frostsnap_core::{
    bitcoin_transaction::{self, LocalSpk},
    tweak::{AppTweakKind, BitcoinAccountKeychain, BitcoinBip32Path},
    MasterAppkey,
};
use frostsnap_core::{
    bitcoin_transaction::{PushInput, TransactionTemplate},
    tweak::BitcoinAccount,
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
pub type WalletUtxo = KeychainIndexed<KeychainId, FullTxOut<ConfirmationBlockTime>>;
pub type WalletTx = CanonicalTx<ConfirmationBlockTime>;

/// Wallet that manages all the frostsnap keys on the same network in a single transaction graph
pub struct CoordSuperWallet {
    tx_graph: Persisted<WalletIndexedTxGraph>,
    chain: Persisted<local_chain::LocalChain>,
    chain_client: ChainClient,
    view: CanonicalView<ConfirmationBlockTime>,
    pub network: bitcoin::Network,
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl CoordSuperWallet {
    fn _update_view(&mut self) {
        self.view = self.tx_graph.canonical_view(
            &*self.chain,
            self.chain.tip().block_id(),
            CanonicalizationParams::default(),
        );
    }

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

        let tx_graph = Persisted::<WalletIndexedTxGraph>::new(&mut *db_, ())
            .context("loading transaction from the database")?;
        let chain = Persisted::<LocalChain>::new(
            &mut *db_,
            bitcoin::constants::genesis_block(network).block_hash(),
        )
        .context("loading chain from database")?;

        drop(db_);

        let view = tx_graph.canonical_view(
            &*chain,
            chain.tip().block_id(),
            CanonicalizationParams::default(),
        );

        Ok(Self {
            tx_graph,
            chain,
            chain_client,
            view,
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
        self.tx_graph
            .index
            .index_of_spk(spk)
            .is_some_and(|((key, _), _)| *key == master_appkey)
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

    fn lazily_initialize_key(&mut self, master_appkey: MasterAppkey) {
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
                let lookahead = self.lookahead();
                let next_index = self
                    .tx_graph
                    .index
                    .last_revealed_index(keychain_id)
                    .map_or(lookahead, |lr| lr + lookahead + 1);
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
                        index: i,
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
                index,
            },
        ))
    }

    fn address_info(&self, master_appkey: MasterAppkey, path: BitcoinBip32Path) -> AddressInfo {
        let keychain = (master_appkey, path.account_keychain);
        let used = self.tx_graph.index.is_used(keychain, path.index);
        let revealed = self.tx_graph.index.last_revealed_index(keychain) <= Some(path.index);
        let spk = super::peek_spk(master_appkey, path);
        AddressInfo {
            index: path.index,
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
                index,
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
                                index: i,
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

    /// Canonical view of all transactions.
    pub fn canonical_view(&self) -> CanonicalView<ConfirmationBlockTime> {
        self.tx_graph.canonical_view(
            &*self.chain,
            self.chain.tip().block_id(),
            CanonicalizationParams::default(),
        )
    }

    /// Determine the tx order of relevant transactions in a canonical `view`.
    pub fn relevant_txs(&mut self, master_appkey: MasterAppkey) -> Vec<WalletTx> {
        self.lazily_initialize_key(master_appkey);
        let mut relevant_txs = self
            .view
            .txs()
            .filter(|canonical_tx| {
                let is_relevant = self
                    .tx_graph
                    .index
                    .txouts_in_tx(canonical_tx.txid)
                    .filter(|(((appkey, _), _), _)| *appkey == master_appkey)
                    .next()
                    .is_some();
                is_relevant
            })
            .collect::<Vec<_>>();
        relevant_txs.sort_unstable_by_key(|tx| core::cmp::Reverse(tx.pos));
        relevant_txs
    }

    pub fn relevant_utxos(
        &mut self,
        master_appkey: MasterAppkey,
    ) -> impl Iterator<Item = WalletUtxo> + '_ {
        self.lazily_initialize_key(master_appkey);
        let owned_outpoints = self
            .tx_graph
            .index
            .keychain_outpoints_in_range(Self::key_index_range(master_appkey));
        self.view.filter_unspent_outpoints(owned_outpoints)
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
                let tx_changeset = tx_graph.apply_update(update.tx_update);
                let changed = !(chain_changeset.is_empty()
                    && indexer_changeset.is_empty()
                    && tx_changeset.is_empty());
                Ok((changed, (tx_changeset, chain_changeset)))
            })?;
        drop(db);
        if changed {
            self._update_view();
        }
        Ok(changed)
    }

    pub fn reconnect(&mut self) {
        self.chain_client.reconnect()
    }

    pub fn send_to(
        &mut self,
        master_appkey: MasterAppkey,
        recipients: impl IntoIterator<Item = (bitcoin::Address, Option<u64>)>,
        feerate: f32,
    ) -> Result<bitcoin_transaction::TransactionTemplate> {
        self.lazily_initialize_key(master_appkey);
        use bdk_coin_select::{
            metrics, Candidate, ChangePolicy, CoinSelector, DrainWeights, FeeRate, Target,
            TargetFee, TargetOutputs, TR_DUST_RELAY_MIN_VALUE, TR_KEYSPEND_TXIN_WEIGHT,
        };

        let recipients = recipients.into_iter().collect::<Vec<_>>();

        let target_outputs = {
            let mut target_outputs = Vec::<bitcoin::TxOut>::with_capacity(recipients.len());
            let mut available_amount = self.calculate_avaliable_value(
                master_appkey,
                recipients.iter().map(|(addr, _)| addr.clone()),
                feerate,
                true,
            );
            for (i, (addr, amount_opt)) in recipients.iter().enumerate() {
                let amount: u64 = match amount_opt {
                    Some(amount) => *amount,
                    None => available_amount
                        .try_into()
                        .map_err(|_| anyhow!("insufficient balance"))?,
                };
                available_amount = available_amount
                    .checked_sub_unsigned(amount)
                    .expect("specified recipient amount is overly large");
                if available_amount < 0 {
                    return Err(anyhow!(
                        "Insufficient balance: {available_amount}sats left for recipient {i}"
                    ));
                }
                target_outputs.push(TxOut {
                    value: Amount::from_sat(amount),
                    script_pubkey: addr.script_pubkey(),
                });
            }
            target_outputs
        };

        let utxos = self.relevant_utxos(master_appkey).collect::<Vec<_>>();

        let candidates = utxos
            .iter()
            .map(|(_path, utxo)| Candidate {
                input_count: 1,
                value: utxo.txout.value.to_sat(),
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                is_segwit: true,
            })
            .collect::<Vec<_>>();

        let target = Target {
            fee: TargetFee::from_feerate(FeeRate::from_sat_per_vb(feerate)),
            outputs: TargetOutputs::fund_outputs(
                target_outputs
                    .iter()
                    .map(|txo| (txo.weight().to_wu(), txo.value.to_sat())),
            ),
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
                event!(Level::ERROR, "unable to find a selection with lowest fee");
                cs.select_until_target_met(target)?;
            }
            Ok(score) => {
                event!(Level::INFO, "coin selection succeeded with score: {score}");
            }
        }

        let selected_utxos = cs.apply_selection(&utxos);

        let mut template_tx = frostsnap_core::bitcoin_transaction::TransactionTemplate::new();

        for (((_master_appkey, account_keychain), index), selected_utxo) in selected_utxos {
            assert_eq!(_master_appkey, &master_appkey);
            let prev_tx = self
                .tx_graph
                .graph()
                .get_tx(selected_utxo.outpoint.txid)
                .expect("must exist");
            let bip32_path = BitcoinBip32Path {
                account_keychain: *account_keychain,
                index: *index,
            };
            template_tx
                .push_owned_input(
                    PushInput::spend_tx_output(prev_tx.as_ref(), selected_utxo.outpoint.vout),
                    LocalSpk {
                        master_appkey,
                        bip32_path,
                    },
                )
                .expect("must be able to add input");
        }

        if let Some(value) = cs.drain_value(target, change_policy) {
            let mut db = self.db.lock().unwrap();
            let (i, _change_spk) = self.tx_graph.mutate(&mut *db, |tx_graph| {
                Ok(tx_graph
                    .index
                    .next_unused_spk((master_appkey, BitcoinAccountKeychain::internal()))
                    .expect(
                        "this should have been initialzed by now since we are spending from it",
                    ))
            })?;

            self.tx_graph
                .MUTATE_NO_PERSIST()
                .index
                .mark_used((master_appkey, BitcoinAccountKeychain::internal()), i);

            template_tx.push_owned_output(
                Amount::from_sat(value),
                LocalSpk {
                    master_appkey,
                    bip32_path: BitcoinBip32Path {
                        account_keychain: BitcoinAccountKeychain::internal(),
                        index: i,
                    },
                },
            );
        }

        for txo in target_outputs {
            template_tx.push_foreign_output(txo);
        }

        Ok(template_tx)
    }

    // TODO: This method should no longer be a method on `CoordSuperWallet` as we no longer need to
    // depend on `self`. Instead, create a separate function elsewhere that takes in
    // `Vec<WalletUtxo>`.
    pub fn calculate_avaliable_value(
        &mut self,
        master_appkey: MasterAppkey,
        target_addresses: impl IntoIterator<Item = bitcoin::Address>,
        feerate: f32,
        effective_only: bool,
    ) -> i64 {
        use bdk_coin_select::{
            Candidate, CoinSelector, Drain, FeeRate, Target, TargetFee, TargetOutputs,
            TR_KEYSPEND_TXIN_WEIGHT,
        };

        let candidates = self
            // `lazy_initialize_key` happens here
            .relevant_utxos(master_appkey)
            .map(|(_path, utxo)| Candidate {
                value: utxo.txout.value.to_sat(),
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                input_count: 1,
                is_segwit: true,
            })
            .collect::<Vec<_>>();

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

        let mut cs = CoinSelector::new(&candidates);
        if effective_only {
            cs.select_all_effective(feerate);
        } else {
            cs.select_all();
        }
        cs.excess(target, Drain::NONE)
    }

    fn key_index_range(
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

    pub fn psbt_to_tx_template(
        &mut self,
        psbt: &bitcoin::Psbt,
        master_appkey: MasterAppkey,
    ) -> Result<TransactionTemplate, PsbtValidationError> {
        let bitcoin_app_xpub = master_appkey.derive_appkey(AppTweakKind::Bitcoin);
        let our_fingerprint = bitcoin_app_xpub.fingerprint();
        let mut template = frostsnap_core::bitcoin_transaction::TransactionTemplate::new();
        let rust_bitcoin_tx = &psbt.unsigned_tx;
        template.set_version(rust_bitcoin_tx.version);
        template.set_lock_time(rust_bitcoin_tx.lock_time);

        let mut already_signed_count = 0;
        let mut foreign_count = 0;
        let mut owned_count = 0;

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
                ($category:ident, $($reason:tt)*) => {{
                    event!(
                        Level::INFO,
                        "Skipping signing PSBT input {i} because it {}", $($reason)*
                    );
                    $category += 1;
                    template.push_foreign_input(input_push);
                    continue;

                }};
            }

            if input.final_script_witness.is_some() {
                bail!(
                    already_signed_count,
                    "it already has a final_script_witness"
                );
            }

            let tap_internal_key = match &input.tap_internal_key {
                Some(tap_internal_key) => tap_internal_key,
                None => bail!(foreign_count, "it doesn't have an tap_internal_key"),
            };

            let (fingerprint, derivation_path) = match input.tap_key_origins.get(tap_internal_key) {
                Some(origin) => origin.1.clone(),
                None => bail!(
                    foreign_count,
                    "it doesn't provide a source for the tap_internal_key"
                ),
            };

            if fingerprint != our_fingerprint {
                bail!(
                    foreign_count,
                    "it's key fingerprint doesn't match our root key"
                );
            }

            let normal_derivation_path = derivation_path
                .into_iter()
                .map(|child_number| match child_number {
                    bip32::ChildNumber::Normal { index } => Ok(*index),
                    _ => Err(anyhow!("can't sign with hardended derivation")),
                })
                .collect::<Result<Vec<_>>>()?;

            let bip32_path = match BitcoinBip32Path::from_u32_slice(&normal_derivation_path) {
                Some(bip32_path) => bip32_path,
                None => {
                    bail!(
                        foreign_count,
                        format!(
                            "it has an unusual derivation path {:?}",
                            normal_derivation_path
                                .into_iter()
                                .map(|n| n.to_string())
                                .collect::<Vec<String>>()
                                .join("/")
                        )
                    );
                }
            };

            template.push_owned_input(
                input_push,
                frostsnap_core::bitcoin_transaction::LocalSpk {
                    master_appkey,
                    bip32_path,
                },
            )?;
            owned_count += 1;
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
                        master_appkey,
                        bip32_path: BitcoinBip32Path {
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

        // Validate that this PSBT is actually signable by this wallet
        if owned_count == 0 {
            return Err(PsbtValidationError::NothingToSign {
                total_inputs: psbt.inputs.len(),
                foreign_count,
                already_signed_count,
            });
        }

        Ok(template)
    }
}

#[derive(Debug, Clone)]
pub enum PsbtValidationError {
    NothingToSign {
        total_inputs: usize,
        foreign_count: usize,
        already_signed_count: usize,
    },
    Other(String),
}

impl From<Box<bitcoin_transaction::SpkDoesntMatchPathError>> for PsbtValidationError {
    fn from(e: Box<bitcoin_transaction::SpkDoesntMatchPathError>) -> Self {
        PsbtValidationError::Other(e.to_string())
    }
}

impl From<anyhow::Error> for PsbtValidationError {
    fn from(e: anyhow::Error) -> Self {
        PsbtValidationError::Other(e.to_string())
    }
}

impl std::fmt::Display for PsbtValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PsbtValidationError::NothingToSign {
                total_inputs,
                foreign_count,
                already_signed_count,
            } => {
                let (input_word, pronoun) = if *total_inputs == 1 {
                    ("input", "it")
                } else {
                    ("inputs", "any of them")
                };

                write!(
                    f,
                    "This PSBT has {total_inputs} {input_word} but this wallet can not sign {pronoun}"
                )?;

                let mut reasons = Vec::new();
                if *foreign_count > 0 {
                    reasons.push(format!("{foreign_count} not owned by this wallet"));
                }
                if *already_signed_count > 0 {
                    reasons.push(format!("{already_signed_count} already signed"));
                }

                if !reasons.is_empty() {
                    write!(f, " ({})", reasons.join(", "))?;
                }

                write!(f, ".")
            }
            PsbtValidationError::Other(msg) => write!(f, "{msg}"),
        }
    }
}
impl std::error::Error for PsbtValidationError {}

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
            index: 42,
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
