//! Building a spend, as the two stages with different rights over the wallet: planning may only
//! read, committing is the single place a change address comes from. The boundary value
//! [`SendPlan`] is a finished coin selection that has reserved nothing, so a caller can plan (and
//! re-plan) as often as it likes — a fee display bound to a plan cannot move the wallet's
//! keychain indices.

use super::wallet::{CoordSuperWallet, KeychainId};
use anyhow::{anyhow, Result};
use bdk_chain::{
    bitcoin::{self, Amount, OutPoint, TxOut},
    indexer::keychain_txout,
    CanonicalizationParams,
};
use bdk_coin_select::{
    metrics, Candidate, ChangePolicy, CoinSelector, DrainWeights, FeeRate, Target, TargetFee,
    TargetOutputs, TR_DUST_RELAY_MIN_VALUE, TR_KEYSPEND_TXIN_WEIGHT,
};
use frostsnap_core::{
    bitcoin_transaction::{LocalSpk, PushInput, TransactionTemplate},
    tweak::{BitcoinAccountKeychain, BitcoinBip32Path, NormalIndex},
    MasterAppkey,
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::{event, Level};

/// A coin a restore with this scan window cannot reach gets force-spent by every plan. From
/// last used index p a restore derives p+1..=p+window and stalls where no history appears; 20
/// is BIP44's gap limit, the strictest convention in common use, and defending it covers every
/// wider window too (bdk's default 25 included) — whatever a 20-window crawl reaches, a wider
/// one reaches a fortiori. It must stay below the wallet's own lookahead — a coin past a bigger
/// gap never enters the UTXO set, so there is nothing to force.
const RISKY_GAP: u32 = 20;

/// A finished coin selection that has reserved nothing.
///
/// Each selected input is pinned as the keychain outpoint it came from — derivation path and
/// outpoint, its immutable identity, plus the value it carried when selected; only its
/// spendability can change. Every input is a taproot keyspend of weight
/// [`TR_KEYSPEND_TXIN_WEIGHT`], and the change output is a value without an address. The plan
/// carries the outputs it was selected FOR and the fee the selection fixed, so the fee it reports
/// is the fee the committed transaction pays. Only [`CoordSuperWallet::commit_send`] turns it into
/// something the wallet is committed to.
#[derive(Debug)]
pub struct SendPlan {
    master_appkey: MasterAppkey,
    selected: Vec<(BitcoinBip32Path, OutPoint, u64)>,
    recipients: Vec<TxOut>,
    change_value: Option<u64>,
    fee: u64,
}

impl SendPlan {
    /// The only way to make a plan, so what a plan means is checked once here instead of trusted
    /// at each site that reads one: the inputs it pins pay for the outputs it carries and the fee
    /// it reports, exactly.
    ///
    /// The check has teeth because the two sides come from different places. A planner derives the
    /// fee from its coin selector's own running total, while `selected` is the input set the plan
    /// will actually be committed with — so this is where the two are made to agree, and a
    /// selection that drifted from the set it priced cannot become a plan.
    fn new(
        master_appkey: MasterAppkey,
        selected: Vec<(BitcoinBip32Path, OutPoint, u64)>,
        recipients: Vec<TxOut>,
        change_value: Option<u64>,
        fee: u64,
    ) -> Result<Self> {
        let inputs: u64 = selected.iter().map(|&(_, _, value)| value).sum();
        let outputs: u64 = recipients.iter().map(|txo| txo.value.to_sat()).sum::<u64>()
            + change_value.unwrap_or(0);
        let spent = outputs
            .checked_add(fee)
            .ok_or_else(|| anyhow!("plan outputs plus fee overflow"))?;
        if inputs != spent {
            return Err(anyhow!(
                "plan does not balance: {inputs} sats of inputs against {outputs} of outputs \
                 and a {fee} sat fee"
            ));
        }
        Ok(Self {
            master_appkey,
            selected,
            recipients,
            change_value,
            fee,
        })
    }

    pub fn master_appkey(&self) -> MasterAppkey {
        self.master_appkey
    }

    pub fn change_value(&self) -> Option<u64> {
        self.change_value
    }

    pub fn fee(&self) -> u64 {
        self.fee
    }

    pub fn input_count(&self) -> usize {
        self.selected.len()
    }

    /// The total value the plan spends: the sum of the inputs it pins, as they stood when
    /// selected. Not a wallet lookup, so it is what the committed transaction consumes even if the
    /// wallet's view moves after planning.
    pub fn input_total(&self) -> u64 {
        self.selected.iter().map(|&(_, _, value)| value).sum()
    }

    /// The coins this plan spends, in selection order.
    pub fn selected_outpoints(&self) -> impl Iterator<Item = OutPoint> + '_ {
        self.selected.iter().map(|&(_, outpoint, _)| outpoint)
    }

    /// What this plan pays recipient `index`, the value the committed transaction carries.
    ///
    /// For send max this is the maximum as the selection fixed it, not as the wallet reports it
    /// now: a deposit landing after planning does not raise it. A display that re-asks the
    /// wallet "what is the max?" shows a number this plan does not pay.
    pub fn recipient_value(&self, index: usize) -> Option<u64> {
        self.recipients.get(index).map(|txo| txo.value.to_sat())
    }
}

impl CoordSuperWallet {
    /// Coin-select for a send. Reads the wallet and reserves nothing, so a display path may call
    /// this freely; only [`commit_send`](Self::commit_send) consumes the result.
    ///
    /// A recipient with a `None` amount receives everything left over (send max).
    pub fn plan_send(
        &mut self,
        master_appkey: MasterAppkey,
        recipients: impl IntoIterator<Item = (bitcoin::Address, Option<u64>)>,
        feerate: f32,
    ) -> Result<SendPlan> {
        self.lazily_initialize_key(master_appkey);

        let recipients = recipients.into_iter().collect::<Vec<_>>();

        let target_outputs = {
            let mut target_outputs = Vec::<TxOut>::with_capacity(recipients.len());
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

        let (keychain_indices, utxos): (Vec<(KeychainId, u32)>, Vec<bdk_chain::FullTxOut<_>>) =
            self.tx_graph
                .graph()
                .filter_chain_unspents(
                    self.chain.as_ref(),
                    self.chain.tip().block_id(),
                    CanonicalizationParams::default(),
                    self.tx_graph
                        .index
                        .keychain_outpoints_in_range(Self::key_index_range(master_appkey)),
                )
                .unzip();

        let candidates = utxos
            .iter()
            .map(|utxo| Candidate {
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
        for position in self.gap_stranded(master_appkey, &keychain_indices) {
            // A coin below its spend cost is the same coin `calculate_avaliable_value` leaves
            // out of the spendable figure, so forcing it would both destroy value and set a
            // target the selection can no longer fund.
            if candidates[position].effective_value(target.fee.rate) > 0.0 {
                cs.select(position);
            }
        }
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

        let selected = cs
            .selected_indices()
            .iter()
            .map(|&position| {
                let ((_, account_keychain), index) = keychain_indices[position];
                (
                    BitcoinBip32Path {
                        account_keychain,
                        index: NormalIndex::new(index)
                            .expect("bdk indexed this utxo against a spk it derived"),
                    },
                    utxos[position].outpoint,
                    utxos[position].txout.value.to_sat(),
                )
            })
            .collect();

        let recipient_value: u64 = target_outputs.iter().map(|txo| txo.value.to_sat()).sum();
        let change_value = cs.drain_value(target, change_policy);
        // Not saturating: a selection that does not cover its own outputs is a bug in the
        // selection, and clamping the fee to zero would hide it behind a plausible number.
        let fee = cs
            .selected_value()
            .checked_sub(recipient_value + change_value.unwrap_or(0))
            .ok_or_else(|| anyhow!("selection does not cover its outputs"))?;
        SendPlan::new(master_appkey, selected, target_outputs, change_value, fee)
    }

    /// Positions in `coins` of ones a [`RISKY_GAP`]-window restore cannot discover, found by
    /// simulating its crawl per keychain: the window starts covering the first RISKY_GAP indices,
    /// and each REACHABLE used index u extends it through u + RISKY_GAP. Reachability is
    /// transitive — history sitting above the window extends nothing, so a whole cluster above
    /// one wide gap is stranded together; measuring only the run below each coin would let
    /// unreachable history reset the gap and leave the cluster's upper coins stranded forever.
    /// "Used" is the indexer's is_used semantics — some txout was indexed there, spent or not —
    /// because spent history feeds a restore's crawl just the same.
    ///
    /// Send max is deliberately not special-cased: its target is every effective coin, so a
    /// stranded coin worth spending is already in the selection and forcing it changes nothing.
    fn gap_stranded(&self, master_appkey: MasterAppkey, coins: &[(KeychainId, u32)]) -> Vec<usize> {
        let mut used: BTreeMap<KeychainId, BTreeSet<u32>> = BTreeMap::new();
        for ((keychain, index), _) in self
            .tx_graph
            .index
            .keychain_outpoints_in_range(Self::key_index_range(master_appkey))
        {
            used.entry(keychain).or_default().insert(index);
        }
        let mut stranded: BTreeMap<KeychainId, BTreeSet<u32>> = BTreeMap::new();
        for (keychain, indices) in &used {
            let unreachable = stranded.entry(*keychain).or_default();
            let mut reach = RISKY_GAP - 1;
            for &index in indices {
                if index <= reach {
                    reach = index + RISKY_GAP;
                } else {
                    unreachable.insert(index);
                }
            }
        }
        coins
            .iter()
            .enumerate()
            .filter(|(_, (keychain, index))| stranded[keychain].contains(index))
            .map(|(position, _)| position)
            .collect()
    }
    /// The coins a [`RISKY_GAP`]-window restore could not discover and that are worth rescuing at
    /// `feerate` — the input set the nudge's remedy consolidates.
    ///
    /// Filtered at the rate the user picked, not at whatever rate made the nudge appear: the
    /// threshold for mentioning a coin and the price of moving it are different questions, and only
    /// this one spends money.
    pub fn gap_stranded_outpoints(
        &mut self,
        master_appkey: MasterAppkey,
        feerate: f32,
    ) -> Vec<OutPoint> {
        self.stranded_rescuable(master_appkey, feerate)
            .into_iter()
            .map(|(_, outpoint, _)| outpoint)
            .collect()
    }

    /// The coins a whole-wallet consolidation spends: every one the wallet holds that is worth more
    /// than it costs to move at `feerate`.
    ///
    /// A coin below its own spend cost is left where it is, because moving it destroys value —
    /// 300 sats carried against roughly 575 of input cost at 10 sat/vB — and forfeits spending it
    /// later at a rate where it does pay. That is the same coin [`Self::plan_send`] declines to
    /// force and [`Self::calculate_avaliable_value`] leaves out of the spendable figure, so
    /// including it here would spend value the wallet does not count as spendable.
    ///
    /// Filtered at the rate the user picked rather than a nominal one: a manual consolidation runs
    /// behind the ordinary feerate picker, so the caller holds the real rate when it composes the
    /// set, and coins enter and leave it as that rate moves.
    pub fn all_unspent_outpoints(
        &mut self,
        master_appkey: MasterAppkey,
        feerate: f32,
    ) -> Vec<OutPoint> {
        let rate = FeeRate::from_sat_per_vb(feerate);
        self.all_unspent(master_appkey)
            .into_iter()
            .filter(|&(_, _, value)| {
                Candidate {
                    input_count: 1,
                    value,
                    weight: TR_KEYSPEND_TXIN_WEIGHT,
                    is_segwit: true,
                }
                .effective_value(rate)
                    > 0.0
            })
            .map(|(_, outpoint, _)| outpoint)
            .collect()
    }

    /// How many coins the wallet holds — what the "Consolidate (n)" menu action shows and gates on
    /// (one coin into one coin is a pure fee burn). A plain count: which of them a consolidation
    /// actually spends depends on the rate the user has not picked yet.
    pub fn utxo_count(&mut self, master_appkey: MasterAppkey) -> usize {
        self.all_unspent(master_appkey).len()
    }

    /// Resolve outpoints the caller named into the identities a plan pins: derivation path and
    /// the value each carries right now. Every one must still be an unspent coin of this key —
    /// a caller planning against a coin the wallet does not hold is working from a stale list,
    /// and saying which coin is what lets it recover.
    ///
    /// A set, not a list: a coin can be spent once, and a repeat would otherwise be priced and
    /// counted twice while producing a transaction that spends the same prevout twice. Nothing
    /// downstream catches that — the doubled value balances against a change output sized to it,
    /// so the plan's own accounting check passes.
    fn owned_unspent(
        &mut self,
        master_appkey: MasterAppkey,
        outpoints: impl IntoIterator<Item = OutPoint>,
    ) -> Result<Vec<(BitcoinBip32Path, OutPoint, u64)>> {
        let held: BTreeMap<OutPoint, (BitcoinBip32Path, u64)> = self
            .all_unspent(master_appkey)
            .into_iter()
            .map(|(path, outpoint, value)| (outpoint, (path, value)))
            .collect();
        let mut named = BTreeSet::new();
        outpoints
            .into_iter()
            .map(|outpoint| {
                if !named.insert(outpoint) {
                    return Err(anyhow!(
                        "{outpoint} was named twice, and a coin spends once"
                    ));
                }
                let &(path, value) = held
                    .get(&outpoint)
                    .ok_or_else(|| anyhow!("{outpoint} is not an unspent coin of this wallet"))?;
                Ok((path, outpoint, value))
            })
            .collect()
    }

    /// Every unspent coin, as consolidation identities.
    fn all_unspent(
        &mut self,
        master_appkey: MasterAppkey,
    ) -> Vec<(BitcoinBip32Path, OutPoint, u64)> {
        self.lazily_initialize_key(master_appkey);
        self.tx_graph
            .graph()
            .filter_chain_unspents(
                self.chain.as_ref(),
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
                self.tx_graph
                    .index
                    .keychain_outpoints_in_range(Self::key_index_range(master_appkey)),
            )
            .map(|(((_, account_keychain), index), utxo)| {
                (
                    BitcoinBip32Path {
                        account_keychain,
                        index: NormalIndex::new(index).expect("bdk never derives a hardened index"),
                    },
                    utxo.outpoint,
                    utxo.txout.value.to_sat(),
                )
            })
            .collect()
    }

    /// The coins the consolidation nudge counts and [`Self::plan_consolidate`] spends — one
    /// source, so the remedy always clears the nudge. A coin qualifies when it is unspent, a
    /// [`RISKY_GAP`]-window restore cannot reach its index, and it is worth more than its own
    /// spend cost at `feerate`: rescuing anything below that eats the coin.
    ///
    /// The rate is the caller's, and the two callers mean different things by it. Deciding whether
    /// to raise the nudge at all is a question about a hypothetical rate, since no rate has been
    /// picked yet; deciding what a rescue spends is a question about the rate the user chose. Only
    /// the second determines what leaves the wallet.
    fn stranded_rescuable(
        &mut self,
        master_appkey: MasterAppkey,
        feerate: f32,
    ) -> Vec<(BitcoinBip32Path, OutPoint, u64)> {
        self.lazily_initialize_key(master_appkey);
        let (keychain_indices, utxos): (Vec<(KeychainId, u32)>, Vec<bdk_chain::FullTxOut<_>>) =
            self.tx_graph
                .graph()
                .filter_chain_unspents(
                    self.chain.as_ref(),
                    self.chain.tip().block_id(),
                    CanonicalizationParams::default(),
                    self.tx_graph
                        .index
                        .keychain_outpoints_in_range(Self::key_index_range(master_appkey)),
                )
                .unzip();
        self.gap_stranded(master_appkey, &keychain_indices)
            .into_iter()
            .filter_map(|position| {
                let candidate = Candidate {
                    input_count: 1,
                    value: utxos[position].txout.value.to_sat(),
                    weight: TR_KEYSPEND_TXIN_WEIGHT,
                    is_segwit: true,
                };
                (candidate.effective_value(FeeRate::from_sat_per_vb(feerate)) > 0.0).then(|| {
                    let ((_, account_keychain), index) = keychain_indices[position];
                    (
                        BitcoinBip32Path {
                            account_keychain,
                            index: NormalIndex::new(index)
                                .expect("bdk never derives a hardened index"),
                        },
                        utxos[position].outpoint,
                        candidate.value,
                    )
                })
            })
            .collect()
    }

    /// How many coins a [`RISKY_GAP`]-window restore could not discover and would be worth
    /// rescuing at `feerate`, and their total value in sats — what the consolidation nudge shows,
    /// and whether it appears at all.
    ///
    /// `feerate` here is a high-water mark rather than a price: nothing is being spent, and the
    /// caller is asking whether a coin's rescue would be worth paying for at an ordinary rate. It
    /// does not constrain what [`Self::gap_stranded_outpoints`] later returns, which answers the
    /// same question at the rate the user actually picked.
    pub fn gap_stranded_value(&mut self, master_appkey: MasterAppkey, feerate: f32) -> (u64, u64) {
        self.stranded_rescuable(master_appkey, feerate)
            .iter()
            .fold((0, 0), |(count, sats), (_, _, value)| {
                (count + 1, sats + value)
            })
    }

    /// Plan a consolidation: every coin the nudge counts goes in, and ONE output — change,
    /// allocated by [`Self::commit_send`] like any other — comes back. There is no recipient
    /// and no coin selection; the input set is the caller's, which is the point. A coin
    /// effective-negative at `feerate` is still included: the caller asked for this coin, and
    /// second-guessing it would leave behind exactly the coin it wanted moved. Refuses when the
    /// coins cannot pay the fee and still leave a usable output.
    ///
    /// Which coins to pass is the caller's question, and the answer the nudge uses is
    /// [`Self::gap_stranded_outpoints`].
    pub fn plan_consolidate(
        &mut self,
        master_appkey: MasterAppkey,
        outpoints: impl IntoIterator<Item = OutPoint>,
        feerate: f32,
    ) -> Result<SendPlan> {
        let coins = self.owned_unspent(master_appkey, outpoints)?;
        if coins.is_empty() {
            return Err(anyhow!("there are no coins to consolidate"));
        }

        let candidates = coins
            .iter()
            .map(|&(_, _, value)| Candidate {
                input_count: 1,
                value,
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                is_segwit: true,
            })
            .collect::<Vec<_>>();
        let mut cs = CoinSelector::new(&candidates);
        for position in 0..candidates.len() {
            cs.select(position);
        }
        let target = Target {
            fee: TargetFee::from_feerate(FeeRate::from_sat_per_vb(feerate)),
            outputs: TargetOutputs::fund_outputs([]),
        };
        let fee = cs.implied_fee(target, DrainWeights::TR_KEYSPEND);
        let sum = cs.selected_value();
        let change_value = sum
            .checked_sub(fee)
            .filter(|value| *value >= TR_DUST_RELAY_MIN_VALUE)
            .ok_or_else(|| {
                anyhow!(
                    "the {} coin(s) hold {sum} sats — not enough to pay the {fee} sat \
                     fee and leave a usable output",
                    coins.len()
                )
            })?;

        SendPlan::new(master_appkey, coins, vec![], Some(change_value), fee)
    }

    /// Turn a [`SendPlan`] into a signable template. This is the wallet's single change-address
    /// allocation point: the lowest revealed-unused index not in `reserved_change`, revealing
    /// fresh only when nothing passes. `reserved_change` is the caller's view of in-flight
    /// reservations (see [`reserved_change_indices`]); the wallet stores nothing. A fresh reveal
    /// cannot collide with it, since reservations come from committed templates whose indices are
    /// at or below the frontier.
    ///
    /// The plan pinned its inputs' immutable identities, so committing re-canonicalizes only
    /// the plan's own outpoints to re-check the one mutable fact: each must still be unspent —
    /// a plan can outlive a sync that spends one of its coins. The plan is dead then; the
    /// caller builds a new one.
    pub fn commit_send(
        &mut self,
        plan: &SendPlan,
        reserved_change: impl IntoIterator<Item = u32>,
    ) -> Result<TransactionTemplate> {
        self.lazily_initialize_key(plan.master_appkey);

        self.owned_unspent(plan.master_appkey, plan.selected_outpoints())
            .map_err(|err| anyhow!("a planned input is no longer spendable: {err}"))?;

        let mut template = TransactionTemplate::new();

        for &(bip32_path, outpoint, _) in &plan.selected {
            let prev_tx = self
                .tx_graph
                .graph()
                .get_tx(outpoint.txid)
                .expect("unspent output implies its tx is in the graph");
            template
                .push_owned_input(
                    PushInput::spend_tx_output(prev_tx.as_ref(), outpoint.vout),
                    LocalSpk {
                        master_appkey: plan.master_appkey,
                        bip32_path,
                    },
                )
                .expect("must be able to add input");
        }

        if let Some(value) = plan.change_value {
            let internal = (plan.master_appkey, BitcoinAccountKeychain::internal());
            let reserved: BTreeSet<u32> = reserved_change.into_iter().collect();
            let mut db = self.db.lock().unwrap();
            let index = self.tx_graph.mutate(&mut db, |tx_graph| {
                let unreserved = tx_graph
                    .index
                    .unused_keychain_spks(internal)
                    .map(|(index, _)| index)
                    .find(|index| !reserved.contains(index));
                Ok(match unreserved {
                    Some(index) => (index, keychain_txout::ChangeSet::default()),
                    None => {
                        let ((index, _spk), changeset) = tx_graph
                            .index
                            .reveal_next_spk(internal)
                            .expect("keychain initialized: we are spending from this wallet");
                        (index, changeset)
                    }
                })
            })?;

            template.push_owned_output(
                Amount::from_sat(value),
                LocalSpk {
                    master_appkey: plan.master_appkey,
                    bip32_path: BitcoinBip32Path {
                        account_keychain: BitcoinAccountKeychain::internal(),
                        index: NormalIndex::new(index)
                            .expect("bdk never reveals a change spk past BIP32_MAX_INDEX"),
                    },
                },
            );
        }

        for txo in &plan.recipients {
            // A recipient can be an address of our own: pasting one is how a user moves coins
            // between their own wallets. Claim it here or the signing device has nothing to name
            // it by, and the user is asked to approve a bare address the app already told them
            // was theirs. Checked because the device derives a claimed output's script from the
            // path rather than reading it here, so a path that did not reproduce this script
            // would move the payment to a different address of ours.
            match self.spk_path(plan.master_appkey, txo.script_pubkey.clone()) {
                Some(bip32_path) => template
                    .push_owned_output_checked(
                        txo,
                        LocalSpk {
                            master_appkey: plan.master_appkey,
                            bip32_path,
                        },
                    )
                    .expect("spk_path resolved this path from this very script"),
                None => template.push_foreign_output(txo.clone()),
            }
        }

        Ok(template)
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::bitcoin::chain_sync::{ChainClient, ConnectionHandler, ElectrumConfig};
    use crate::bitcoin::wallet::CoordSuperWallet;
    use crate::persist::Persisted;
    use crate::settings::ElectrumEnabled;
    use bdk_chain::{
        bitcoin::{hashes::Hash, BlockHash, TxIn},
        BlockId, CheckPoint, ConfirmationBlockTime, TxUpdate,
    };
    use bdk_coin_select::Drain;
    use frostsnap_core::schnorr_fun::fun::Point;
    use frostsnap_core::tweak::NormalIndex;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    const NETWORK: bitcoin::Network = bitcoin::Network::Bitcoin;

    /// The rate the app raises the nudge at: worth mentioning a coin whose rescue pays for itself
    /// at an ordinary feerate. It is the app's number, not the wallet's — these tests pass it
    /// explicitly for the same reason the Dart caller does.
    const NUDGE_BAR: f32 = 10.0;

    /// The handler owns the receiving ends of the client's channels, so it must outlive every
    /// `ChainClient` call or `monitor_keychain`'s send panics.
    fn chain_client(db: &Arc<Mutex<rusqlite::Connection>>) -> (ChainClient, ConnectionHandler) {
        let trusted = {
            let mut conn = db.lock().unwrap();
            Persisted::new(&mut *conn, NETWORK).unwrap()
        };
        ChainClient::new(
            NETWORK,
            ElectrumConfig {
                enabled: ElectrumEnabled::None,
                primary: String::new(),
                backup: String::new(),
            },
            trusted,
            db.clone(),
        )
    }

    struct Fixture {
        wallet: CoordSuperWallet,
        handler: ConnectionHandler,
        master_appkey: MasterAppkey,
        recipient: bitcoin::Address,
        blocks: Vec<BlockId>,
    }

    impl Fixture {
        fn new() -> Self {
            let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
            let (client, handler) = chain_client(&db);
            let master_appkey =
                MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
            let mut wallet = CoordSuperWallet::load_or_init(db, NETWORK, client).unwrap();
            wallet.list_addresses(master_appkey);
            let recipient =
                bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
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
        fn fund(&mut self, index: u32, value: u64, height: u32) -> OutPoint {
            self.fund_keychain(BitcoinAccountKeychain::external(), index, value, height)
        }

        /// Deliver a confirmed payment to either keychain. Change lands on the internal one,
        /// which is where the burned indices are.
        fn fund_keychain(
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
        fn spend_paying_unannounced_change(
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

        fn last_revealed_internal(&self) -> Option<u32> {
            self.wallet
                .tx_graph
                .index
                .last_revealed_index((self.master_appkey, BitcoinAccountKeychain::internal()))
        }

        fn plan_all(&mut self, feerate: f32) -> Result<SendPlan> {
            let outpoints = self
                .wallet
                .all_unspent_outpoints(self.master_appkey, feerate);
            self.wallet
                .plan_consolidate(self.master_appkey, outpoints, feerate)
        }

        /// The input set the nudge's remedy consolidates, composed the way its call site does.
        fn plan_stranded(&mut self, feerate: f32) -> Result<SendPlan> {
            let outpoints = self
                .wallet
                .gap_stranded_outpoints(self.master_appkey, feerate);
            self.wallet
                .plan_consolidate(self.master_appkey, outpoints, feerate)
        }

        fn plan(&mut self, sats: u64) -> SendPlan {
            self.wallet
                .plan_send(
                    self.master_appkey,
                    [(self.recipient.clone(), Some(sats))],
                    1.0,
                )
                .unwrap()
        }
    }

    /// The feature, in the shape that produced it. 0.3 could pay change far past the frontier it
    /// persisted; the transaction is held anyway because we own a spent prevout, and nothing in the
    /// update points at the change. Attributing it is purely a matter of having derived that far, so
    /// this fails at the old lookahead of 50 and passes at `LOOKAHEAD`.
    #[test]
    fn change_the_server_never_named_is_attributed_and_spendable() {
        let mut f = Fixture::new();
        let coin = f.fund(0, 100_000, 100);
        f.spend_paying_unannounced_change(coin, 800, 60_000, 101);

        assert_eq!(
            f.last_revealed_internal(),
            Some(800),
            "the frontier must land on the index that was actually paid"
        );
        // The funding coin is spent, so the recovered change is the only thing a plan can spend.
        let plan = f.plan(20_000);
        assert_eq!(plan.input_count(), 1);
        assert_eq!(plan.input_total(), 60_000);
    }

    /// `LOOKAHEAD` is a parameter, not a boundary: nothing records that a database has been searched,
    /// so a coin the shipped constant passes over is still there for a larger one to find. That is
    /// what makes raising it in a later release a repair rather than a no-op, and it is the one
    /// property that cannot be checked end to end — a drive cannot vary a compiled-in constant.
    #[test]
    fn a_wider_lookahead_finds_what_the_shipped_one_passed_over() {
        let beyond = crate::bitcoin::wallet::LOOKAHEAD + 500;
        let mut f = Fixture::new();
        let coin = f.fund(0, 100_000, 100);
        f.spend_paying_unannounced_change(coin, beyond, 60_000, 101);
        assert_eq!(
            f.last_revealed_internal(),
            None,
            "beyond the window there is nothing to attribute it with"
        );

        let db = f.wallet.db.clone();
        let (client, _handler) = chain_client(&db);
        let mut wider =
            CoordSuperWallet::load_or_init_with_lookahead(db, NETWORK, client, beyond + 10)
                .unwrap();
        // Touching the key is what rebuilds attribution, and the wider window is what lets that
        // pass recognise the change.
        wider.lazily_initialize_key(f.master_appkey);

        // Spendability, not the frontier: the funding coin is spent, so a plan that balances at all
        // is paying from change the narrower window could not see.
        let plan = wider
            .plan_send(f.master_appkey, [(f.recipient.clone(), Some(20_000))], 1.0)
            .unwrap();
        assert_eq!(plan.input_count(), 1);
        assert_eq!(
            plan.input_total(),
            60_000,
            "the same database, searched further, gives the coin back"
        );
    }

    /// The guard the constructor exists for. A plan whose pinned inputs do not pay for what it
    /// says it pays is not a plan, and before the values were carried this was unrepresentable:
    /// `input_total` was derived from the outputs, so such a plan reported the amount it should
    /// have been spending while pinning nothing that could pay it.
    #[test]
    fn a_plan_whose_inputs_do_not_cover_its_outputs_is_refused() {
        let f = Fixture::new();
        let input = (
            BitcoinBip32Path {
                account_keychain: BitcoinAccountKeychain::external(),
                index: NormalIndex::new(0).unwrap(),
            },
            OutPoint::null(),
            10_000,
        );
        let recipient = |value| {
            vec![TxOut {
                value: Amount::from_sat(value),
                script_pubkey: f.recipient.script_pubkey(),
            }]
        };

        SendPlan::new(f.master_appkey, vec![input], recipient(9_000), None, 1_000)
            .expect("10,000 in, 9,000 out, 1,000 fee");

        let err = SendPlan::new(f.master_appkey, vec![input], recipient(9_500), None, 1_000)
            .expect_err("9,500 out and a 1,000 fee cannot come from 10,000");
        assert!(err.to_string().contains("does not balance"), "got: {err}");
    }

    #[test]
    fn planning_reserves_nothing() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);

        for _ in 0..10 {
            f.plan(10_000);
        }
        assert_eq!(
            f.last_revealed_internal(),
            None,
            "a fee display re-planning every frame must not move the change keychain"
        );
    }

    #[test]
    fn commit_pays_the_planned_fee_from_the_first_change_index() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);

        let plan = f.plan(10_000);
        let planned_fee = plan.fee();
        let template = f.wallet.commit_send(&plan, []).unwrap();

        assert_eq!(template.fee(), Some(planned_fee), "fee shown is fee paid");
        assert_eq!(
            f.last_revealed_internal(),
            Some(0),
            "the first send's change uses the first change address"
        );
    }

    #[test]
    fn consecutive_broadcast_sends_use_consecutive_change_indices() {
        let mut f = Fixture::new();
        for (i, height) in (0..3).zip(100..) {
            f.fund(i, 1_000_000, height);
        }

        for expected_index in 0..3 {
            // The signers page re-plans on every rebuild; none of that may widen the gap.
            for _ in 0..5 {
                f.plan(10_000);
            }
            let plan = f.plan(10_000);
            let template = f.wallet.commit_send(&plan, []).unwrap();
            assert_eq!(f.last_revealed_internal(), Some(expected_index));
            // Broadcasting puts the tx in the graph, where scanning marks its change index used
            // durably — that, not any in-RAM reservation, is what advances allocation.
            f.wallet.broadcast_success(template.to_rust_bitcoin_tx());
        }
    }

    /// Until a committed tx reaches the graph, a repeat send picks the SAME change index — reuse
    /// between our own in-flight txs, accepted as the rarer, funds-safe failure over a RAM-only
    /// reservation that burned cancelled sends' indices and died on restart regardless.
    #[test]
    fn unbroadcast_sends_reuse_the_change_index() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);

        for _ in 0..2 {
            let plan = f.plan(10_000);
            f.wallet.commit_send(&plan, []).unwrap();
            assert_eq!(f.last_revealed_internal(), Some(0));
        }
    }

    /// The counterpart of the baseline above: hand commit_send the pending send's index as
    /// reserved and the next send advances instead of reusing it.
    #[test]
    fn a_pending_sends_change_index_is_skipped() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);

        let plan = f.plan(10_000);
        f.wallet.commit_send(&plan, []).unwrap();
        let plan = f.plan(10_000);
        let template = f.wallet.commit_send(&plan, [0]).unwrap();
        let change_index = template
            .as_seen_by(plan.master_appkey)
            .iter_our_outputs()
            .map(|(_, _, spk)| spk.bip32_path.index.to_u32())
            .next()
            .expect("send has a change output");
        assert_eq!(change_index, 1);
    }

    /// The external-keychain indices the policy would force-spend, straight off the fixture.
    fn stranded_external_indices(f: &Fixture) -> Vec<u32> {
        let w = &f.wallet;
        let (keychain_indices, _): (Vec<(KeychainId, u32)>, Vec<bdk_chain::FullTxOut<_>>) =
            w.tx_graph
                .graph()
                .filter_chain_unspents(
                    w.chain.as_ref(),
                    w.chain.tip().block_id(),
                    CanonicalizationParams::default(),
                    w.tx_graph.index.keychain_outpoints_in_range(
                        CoordSuperWallet::key_index_range(f.master_appkey),
                    ),
                )
                .unzip();
        w.gap_stranded(f.master_appkey, &keychain_indices)
            .into_iter()
            .map(|position| keychain_indices[position].1)
            .collect()
    }

    /// The first index a BIP44-window restore cannot discover is 21 past the last used one (20
    /// never-used below it) — an ordinary send must consolidate the coin sitting there even
    /// though the target needs nothing from it.
    #[test]
    fn an_ordinary_send_consolidates_gap_stranded_coins() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let stranded = f.fund(21, 500_000, 101);

        let plan = f.plan(10_000);
        assert!(
            plan.selected_outpoints().any(|op| op == stranded),
            "the coin past the risky gap must be force-spent"
        );
        f.wallet.commit_send(&plan, []).unwrap();
    }

    /// One index closer and a restore still finds it: a gap of 19 forces nothing.
    #[test]
    fn a_lookahead_safe_gap_is_not_forced() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        f.fund(20, 500_000, 101);
        assert_eq!(stranded_external_indices(&f), Vec::<u32>::new());
    }

    /// Recovery is transitive: history a restore cannot reach extends nothing. With coins at 30
    /// and 45 above one wide gap, BOTH are invisible to a restore stalled at the gap — forcing
    /// only 30 (whose local run is wide) and not 45 (whose local run is 14) would leave 45
    /// stranded forever, since 30's spent history keeps 45's local run short after the sweep.
    #[test]
    fn gap_stranded_reports_a_whole_cluster() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        f.fund(30, 500_000, 101);
        f.fund(45, 500_000, 102);
        assert_eq!(stranded_external_indices(&f), vec![30, 45]);
    }

    /// A spent-through index still shows history to a restore's crawl, so it resets the gap
    /// here too: with index 13 spent, the crawl chains 0 → 13 → 26 in sub-window hops.
    #[test]
    fn spent_history_resets_the_gap() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let spent = f.fund(13, 200_000, 101);
        let spend = bitcoin::Transaction {
            version: bitcoin::transaction::Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: spent,
                ..Default::default()
            }],
            output: vec![TxOut {
                value: Amount::from_sat(150_000),
                script_pubkey: bitcoin::ScriptBuf::new_op_return([]),
            }],
        };
        f.wallet.broadcast_success(spend);
        f.fund(26, 500_000, 102);
        assert_eq!(stranded_external_indices(&f), Vec::<u32>::new());
    }

    /// The internal keychain is where the burned change indices are, so scattered change is
    /// what a restore cannot reach. The policy has to defend it exactly as it defends receive
    /// addresses.
    #[test]
    fn a_stranded_change_coin_is_forced_too() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let stranded_change = f.fund_keychain(BitcoinAccountKeychain::internal(), 21, 500_000, 101);

        let plan = f.plan(10_000);
        assert!(
            plan.selected_outpoints().any(|op| op == stranded_change),
            "change past the risky gap must be force-spent too"
        );
    }

    /// The nudge and the rescue must agree: the summary counts exactly the coins a plan would
    /// force, so a banner built on it can always be satisfied by one send.
    #[test]
    fn gap_stranded_value_counts_only_rescuable_coins() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (0, 0)
        );

        f.fund(30, 500_000, 101);
        // Stranded and above its spend cost at the 1 sat/vB relay floor, but below it at the
        // summary's 10 sat/vB bar — pins that the bar is 10, not merely relayable.
        f.fund(45, 300, 102);
        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (1, 500_000)
        );
    }

    /// The plan spends the nudge's exact coin set — nothing reachable, nothing extra — into a
    /// single output, and planning commits the wallet to nothing.
    #[test]
    fn consolidation_spends_exactly_the_nudged_coins_into_one_change_output() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let stranded_ext = f.fund(30, 500_000, 101);
        let stranded_int = f.fund_keychain(BitcoinAccountKeychain::internal(), 25, 200_000, 102);
        let revealed_before = f.last_revealed_internal();

        let plan = f.plan_stranded(10.0).unwrap();

        let planned: BTreeSet<OutPoint> = plan.selected_outpoints().collect();
        assert_eq!(planned, BTreeSet::from([stranded_ext, stranded_int]));
        assert!(plan.recipients.is_empty());
        let change = plan.change_value.expect("the one output is change");
        assert_eq!(
            plan.fee + change,
            700_000,
            "every input sat is accounted for"
        );

        let candidates = [
            Candidate {
                input_count: 1,
                value: 500_000,
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                is_segwit: true,
            },
            Candidate {
                input_count: 1,
                value: 200_000,
                weight: TR_KEYSPEND_TXIN_WEIGHT,
                is_segwit: true,
            },
        ];
        let mut cs = CoinSelector::new(&candidates);
        cs.select(0);
        cs.select(1);
        let implied = cs
            .implied_feerate(
                TargetOutputs::fund_outputs([]),
                Drain {
                    weights: DrainWeights::TR_KEYSPEND,
                    value: change,
                },
            )
            .expect("valid selection")
            .as_sat_vb();
        assert!(
            (10.0..10.5).contains(&implied),
            "the plan pays the requested feerate, got {implied} sat/vB"
        );

        assert_eq!(
            f.last_revealed_internal(),
            revealed_before,
            "planning must not reveal"
        );
    }

    /// What an outpoint argument opened up that a wallet-chosen input set could not: naming a coin
    /// twice. Nothing downstream catches it — the doubled value is matched by a change output
    /// sized to it, so the plan balances — and committing would spend one prevout twice.
    #[test]
    fn a_coin_named_twice_is_refused() {
        let mut f = Fixture::new();
        let coin = f.fund(0, 1_000_000, 100);
        let other = f.fund(1, 1_000_000, 101);

        let err = f
            .wallet
            .plan_consolidate(f.master_appkey, [coin, coin], 10.0)
            .expect_err("a coin spends once");
        assert!(err.to_string().contains("named twice"), "got: {err}");

        let plan = f
            .wallet
            .plan_consolidate(f.master_appkey, [coin, other], 10.0)
            .expect("two distinct coins are fine");
        assert_eq!(plan.input_total(), 2_000_000);
    }

    /// The first of two refusals: the coins cannot cover the fee at all.
    #[test]
    fn consolidation_refuses_when_the_coins_cannot_pay_the_fee() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        f.fund(21, 700, 101); // above the nudge bar, below one input+output of fee at 10 sat/vB

        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (1, 700)
        );
        let err = f
            .plan_stranded(10.0)
            .expect_err("700 sats cannot fund the consolidation");
        assert!(err.to_string().contains("not enough"), "got: {err}");
    }

    /// The second: the coins do cover the fee, and what is left is under the dust floor. Its own
    /// test because the two refusals answer with one message, and the case above reaches this one's
    /// branch never — it fails the subtraction, so the floor could be deleted without it noticing.
    #[test]
    fn consolidation_refuses_a_dust_change_output() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        // Covers the 1,110 sat fee and leaves 290: a real output, under the 330 sat relay floor.
        f.fund(21, 1_400, 101);
        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (1, 1_400)
        );

        let err = f
            .plan_stranded(10.0)
            .expect_err("290 sats of change is dust");
        assert!(err.to_string().contains("not enough"), "got: {err}");

        // Same coin, same fee, 490 left: the refusal above was the floor and not a shortfall.
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        f.fund(21, 1_600, 101);
        let plan = f.plan_stranded(10.0).unwrap();
        assert_eq!(plan.change_value, Some(490));
    }

    /// Committing flows through the ordinary change allocation, and broadcasting the result is
    /// the remedy the nudge promised: the stranded set empties.
    #[test]
    fn committed_consolidation_clears_the_nudge() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        f.fund(30, 500_000, 101);
        f.fund(60, 400_000, 102);
        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (2, 900_000)
        );

        let plan = f.plan_stranded(10.0).unwrap();
        let template = f.wallet.commit_send(&plan, []).unwrap();
        assert_eq!(template.fee(), Some(plan.fee), "fee shown is fee paid");
        assert_eq!(
            f.last_revealed_internal(),
            Some(0),
            "the single output rides the ordinary change allocation"
        );

        let tx = template.to_rust_bitcoin_tx();
        assert_eq!(tx.output.len(), 1, "consolidation has exactly one output");
        assert_eq!(tx.output[0].value.to_sat(), plan.change_value.unwrap());
        let spent: BTreeSet<OutPoint> = tx.input.iter().map(|txin| txin.previous_output).collect();
        assert_eq!(spent, plan.selected_outpoints().collect::<BTreeSet<_>>());

        f.wallet.broadcast_success(tx);
        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (0, 0),
            "the remedy clears the nudge"
        );
    }

    /// A coin the nudge mentioned but that costs more to move than it carries at the rate the user
    /// picked is left where it is. The nudge's bar is a hypothetical — worth telling you about — and
    /// this is the real price.
    ///
    /// The trade-off is deliberate and has a cost worth naming: the remedy does not clear the nudge
    /// here. The coin stays out of a restore's reach and the banner keeps saying so, correctly,
    /// because at this rate moving it would burn more than it holds. A later consolidation at a
    /// lower rate rescues it.
    #[test]
    fn a_coin_not_worth_moving_at_the_chosen_rate_is_left_where_it_is() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let worth_moving = f.fund(30, 500_000, 101);
        // ~575 sats of input cost at 10 sat/vB, ~2,875 at 50: above the nudge's bar, under the
        // price of moving it at the rate this plan pays.
        f.fund(51, 1_500, 102);
        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (2, 501_500),
            "the nudge counts it, because at an ordinary rate its rescue pays for itself"
        );

        let plan = f.plan_stranded(50.0).unwrap();
        assert_eq!(
            plan.selected_outpoints().collect::<BTreeSet<_>>(),
            BTreeSet::from([worth_moving]),
            "only the coin whose rescue is worth its own fee at this rate"
        );

        let template = f.wallet.commit_send(&plan, []).unwrap();
        f.wallet.broadcast_success(template.to_rust_bitcoin_tx());
        assert_eq!(
            f.wallet.gap_stranded_value(f.master_appkey, NUDGE_BAR),
            (1, 1_500),
            "so the nudge survives its own remedy, still naming the coin left behind"
        );
    }

    /// Whole-wallet consolidation: every coin worth moving at the chosen rate goes into one
    /// output, and one that is not stays where it is. Cleaning up should not cost more than the
    /// mess — a coin carrying less than its own input cost makes the user poorer by merging it, and
    /// forfeits spending it later at a rate where it pays.
    #[test]
    fn all_scope_leaves_behind_what_costs_more_than_it_carries() {
        let mut f = Fixture::new();
        let reachable = f.fund(0, 1_000_000, 100);
        let stranded = f.fund(30, 500_000, 101);
        let dust = f.fund(1, 1_500, 102); // ~2,875 sats of input cost at the plan rate
        assert_eq!(f.wallet.utxo_count(f.master_appkey), 3);
        let revealed_before = f.last_revealed_internal();

        let plan = f.plan_all(50.0).unwrap();
        assert_eq!(
            f.last_revealed_internal(),
            revealed_before,
            "planning must not reveal"
        );
        let planned: BTreeSet<OutPoint> = plan.selected_outpoints().collect();
        assert_eq!(planned, BTreeSet::from([reachable, stranded]));
        assert!(
            !planned.contains(&dust),
            "merging it would cost more than it carries"
        );
        assert!(plan.recipients.is_empty());
        assert_eq!(
            plan.fee + plan.change_value.unwrap(),
            1_500_000,
            "every input sat is accounted for"
        );
    }

    /// The planner prices the set it is given and forms no opinion about it. Moving one coin into
    /// one coin is a pure fee burn for a coin already within reach and the entire point for one a
    /// restore would miss — a difference only the caller knows it is making, and both callers gate
    /// on it themselves: the menu hides the action below two coins, and the banner appears only
    /// when something is stranded.
    #[test]
    fn a_single_coin_is_planned_because_the_caller_asked_for_it() {
        let mut f = Fixture::new();
        f.fund(0, 500_000, 100); // reachable: pointless to move, and planned anyway
        assert_eq!(f.wallet.utxo_count(f.master_appkey), 1);
        let plan = f.plan_all(10.0).expect("the planner does what it is told");
        assert_eq!(plan.input_count(), 1);

        let mut f = Fixture::new();
        f.fund(30, 500_000, 100); // past RISKY_GAP, so a restore would miss it
        f.plan_all(10.0)
            .expect("and the same for the coin where moving it is the point");
        f.plan_stranded(10.0)
            .expect("both paths plan the same lone coin");
    }

    /// The shortfall guard is scope-independent, and says so without calling reachable coins
    /// stranded.
    #[test]
    fn all_scope_refuses_when_the_coins_cannot_pay_the_fee() {
        let mut f = Fixture::new();
        // 700 each: above the ~575 sat cost of its own input at this rate, so both survive into
        // the set, and 1,400 together is still under the ~1,685 sat fee for two inputs.
        f.fund(0, 700, 100);
        f.fund(1, 700, 101);
        let err = f
            .plan_all(10.0)
            .expect_err("1,400 sats cannot fund two inputs and an output");
        assert!(err.to_string().contains("not enough"), "got: {err}");
        assert!(
            !err.to_string().contains("stranded"),
            "reachable coins must not be described as stranded: {err}"
        );
    }

    /// Rescue must not cost the user their send. A coin worth less than it costs to spend is
    /// left out of the spendable figure, so forcing it would set a target the selection cannot
    /// fund and send max would fail outright on a wallet with a full balance.
    #[test]
    fn a_stranded_coin_below_its_spend_cost_is_left_alone() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let dust = f.fund(21, 300, 101); // past the risky gap, under its own spend cost here

        let available =
            f.wallet
                .calculate_avaliable_value(f.master_appkey, [f.recipient.clone()], 10.0, true);
        let plan = match f
            .wallet
            .plan_send(f.master_appkey, [(f.recipient.clone(), None)], 10.0)
        {
            Ok(plan) => plan,
            Err(e) => panic!("wallet advertises {available} spendable but send max fails: {e}"),
        };
        assert!(
            !plan.selected_outpoints().any(|op| op == dust),
            "moving it costs more than it carries, so it stays where it is"
        );
    }

    #[test]
    fn send_max_plans_have_no_change_and_commit_allocates_nothing() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);

        let plan = f
            .wallet
            .plan_send(f.master_appkey, [(f.recipient.clone(), None)], 1.0)
            .unwrap();
        assert_eq!(plan.change_value, None);

        f.wallet.commit_send(&plan, []).unwrap();
        assert_eq!(f.last_revealed_internal(), None);
    }

    /// A send-max plan's amount is fixed when the plan is. A deposit landing while the user picks
    /// signers must not move it, so the review screen has to read the amount from the plan — the
    /// wallet's live answer to "what is the max?" is a number this transaction does not pay.
    #[test]
    fn a_deposit_after_planning_does_not_move_a_send_max_amount() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);

        let plan = f
            .wallet
            .plan_send(f.master_appkey, [(f.recipient.clone(), None)], 1.0)
            .unwrap();
        let planned = plan.recipient_value(0).expect("the one recipient");

        f.fund(1, 5_000_000, 101);
        assert_eq!(
            plan.recipient_value(0),
            Some(planned),
            "the plan is a snapshot, not a view of the wallet"
        );

        let paid = f
            .wallet
            .commit_send(&plan, [])
            .unwrap()
            .to_rust_bitcoin_tx()
            .output
            .iter()
            .find(|txo| txo.script_pubkey == f.recipient.script_pubkey())
            .expect("the recipient is paid")
            .value
            .to_sat();
        assert_eq!(paid, planned, "the amount shown is the amount paid");
    }

    /// A reveal the wallet only learned about from a sync must survive a restart, and the coin it
    /// makes visible must stay spendable. The reveal was the exact changeset `apply_update`
    /// discarded, so every reload re-indexed stored txs against a too-narrow window and the coin
    /// showed in the balance but never in coin selection.
    #[test]
    fn a_synced_reveal_survives_a_restart_and_its_coin_stays_spendable() {
        const VALUE: u64 = 1_000_000;

        let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
        let master_appkey =
            MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
        let recipient = bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
            .unwrap()
            .require_network(NETWORK)
            .unwrap();
        let external = (master_appkey, BitcoinAccountKeychain::external());
        let blocks = [
            BlockId {
                height: 0,
                hash: bitcoin::constants::genesis_block(NETWORK).block_hash(),
            },
            BlockId {
                height: 100,
                hash: BlockHash::from_byte_array([100u8; 32]),
            },
        ];

        // First run: a sync reports activity at an index the wallet had not revealed itself.
        let far;
        {
            let (client, _handler) = chain_client(&db);
            let mut wallet = CoordSuperWallet::load_or_init(db.clone(), NETWORK, client).unwrap();
            wallet.list_addresses(master_appkey);

            // Past the window the wallet re-derives at load, so only the persisted reveal can
            // bring the coin back: inside it the load-time re-index finds the coin unaided and the
            // test would pass with or without the fix. Read off the wallet rather than written as
            // a literal, so widening the lookahead cannot quietly make this vacuous.
            far = wallet.lookahead() + 10;

            let tx = bitcoin::Transaction {
                version: bitcoin::transaction::Version::TWO,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![TxIn::default()],
                output: vec![TxOut {
                    value: Amount::from_sat(VALUE),
                    script_pubkey: crate::bitcoin::peek_spk(
                        master_appkey,
                        BitcoinBip32Path {
                            account_keychain: BitcoinAccountKeychain::external(),
                            index: NormalIndex::new(far)
                                .expect("far is lookahead + 10, well inside the normal range"),
                        },
                    ),
                }],
            };
            let mut tx_update = TxUpdate::default();
            tx_update.txs = vec![Arc::new(tx.clone())];
            tx_update.anchors = [(
                ConfirmationBlockTime {
                    block_id: blocks[1],
                    confirmation_time: 1_700_000_000,
                },
                tx.compute_txid(),
            )]
            .into();

            wallet
                .apply_update(bdk_electrum_streaming::Update {
                    tx_update,
                    last_active_indices: [(external, far)].into(),
                    chain_update: Some(CheckPoint::from_block_ids(blocks).unwrap()),
                })
                .unwrap();

            assert!(
                wallet.calculate_avaliable_value(master_appkey, [recipient.clone()], 1.0, true) > 0,
                "the coin is spendable in the session that discovered it"
            );
        }

        // Restart: same database, fresh wallet.
        let (client, _handler) = chain_client(&db);
        let mut wallet = CoordSuperWallet::load_or_init(db.clone(), NETWORK, client).unwrap();
        wallet.list_addresses(master_appkey);

        assert_eq!(
            wallet.tx_graph.index.last_revealed_index(external),
            Some(far),
            "the frontier the sync established must be on disk"
        );
        let available = wallet.calculate_avaliable_value(master_appkey, [recipient], 1.0, true);
        assert!(
            available > 0,
            "balance and send max must agree across a restart; send max saw {available}"
        );
    }

    #[test]
    fn a_plan_outlived_by_its_coins_errors_instead_of_committing() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let plan = f.plan(10_000);

        // A sync spends the planned coin out from under the plan.
        let coin = f.wallet.get_tx(plan.selected[0].1.txid).unwrap();
        let spend = bitcoin::Transaction {
            version: bitcoin::transaction::Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn {
                previous_output: plan.selected[0].1,
                ..Default::default()
            }],
            output: vec![TxOut {
                value: coin.output[0].value - Amount::from_sat(500),
                script_pubkey: bitcoin::ScriptBuf::new_op_return([]),
            }],
        };
        let block = BlockId {
            height: 101,
            hash: BlockHash::from_byte_array([101u8; 32]),
        };
        let mut tx_update = TxUpdate::default();
        tx_update.txs = vec![Arc::new(spend.clone())];
        tx_update.anchors = [(
            ConfirmationBlockTime {
                block_id: block,
                confirmation_time: 1_700_000_100,
            },
            spend.compute_txid(),
        )]
        .into();
        f.blocks.push(block);
        f.wallet
            .apply_update(bdk_electrum_streaming::Update {
                tx_update,
                last_active_indices: Default::default(),
                chain_update: Some(CheckPoint::from_block_ids(f.blocks.iter().copied()).unwrap()),
            })
            .unwrap();

        let err = f.wallet.commit_send(&plan, []).unwrap_err();
        assert!(
            err.to_string().contains("no longer spendable"),
            "the one real failure mode is a spent planned input: {err}"
        );
        assert_eq!(
            f.last_revealed_internal(),
            None,
            "a failed commit must not allocate"
        );

        // Staleness is recoverable: a fresh plan over what is actually left commits fine.
        f.fund(1, 1_000_000, 102);
        let fresh = f.plan(10_000);
        f.wallet.commit_send(&fresh, []).unwrap();
        assert_eq!(f.last_revealed_internal(), Some(0));
    }

    /// The asymmetry's obligation. The indexer reaches a frontier the server was never asked
    /// about, so a reveal we derived locally has to become a subscription — otherwise the coin is
    /// spendable while its own spend goes unseen.
    #[test]
    fn a_frontier_reached_locally_is_pushed_to_the_chain_source() {
        use bdk_electrum_streaming::ClientAction;

        let mut f = Fixture::new();
        let keychain = BitcoinAccountKeychain::internal();
        let far = 400; // past SUBSCRIPTION_LOOKAHEAD, so only a fresh descriptor covers it
        f.fund_keychain(keychain, far, 60_000, 100);

        let asked = f.handler.drain_tracked().into_iter().fold(
            BTreeMap::<KeychainId, u32>::new(),
            |mut widest, action| {
                if let ClientAction::AddDescriptor {
                    keychain,
                    next_index,
                    ..
                } = action
                {
                    let entry = widest.entry(keychain).or_default();
                    *entry = (*entry).max(next_index);
                }
                widest
            },
        );

        assert_eq!(
            asked.get(&(f.master_appkey, keychain)).copied(),
            Some(far + 1),
            "the keychain that revealed locally is the one the server must be told about: {asked:?}"
        );
    }

    /// Pasting one of our own addresses is how a user moves coins between their own wallets. The
    /// app labels such a recipient off the same index the wallet derives it at, so a template
    /// that hands the device a bare foreign script leaves the two surfaces disagreeing about
    /// what is being approved.
    #[test]
    fn a_recipient_we_derive_is_claimed_so_the_device_can_name_it() {
        let mut f = Fixture::new();
        f.fund(0, 1_000_000, 100);
        let own_path = BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain::external(),
            index: NormalIndex::new(7).unwrap(),
        };
        let own_address = bitcoin::Address::from_script(
            &crate::bitcoin::peek_spk(f.master_appkey, own_path),
            NETWORK,
        )
        .unwrap();

        let plan = f
            .wallet
            .plan_send(f.master_appkey, [(own_address, Some(500_000))], 1.0)
            .unwrap();
        let template = f.wallet.commit_send(&plan, []).unwrap();
        let prompt = template.as_seen_by(f.master_appkey).user_prompt(NETWORK);

        // Change first, then the recipient, both ours: with nothing foreign left to contrast it
        // against, the change output stops being the skippable half of an ordinary send and is
        // disclosed too.
        assert_eq!(
            prompt
                .recipients
                .iter()
                .map(|r| (r.amount.to_sat(), r.owned))
                .collect::<Vec<_>>(),
            vec![
                (
                    plan.change_value.expect("send has change"),
                    Some(BitcoinBip32Path {
                        account_keychain: BitcoinAccountKeychain::internal(),
                        index: NormalIndex::ZERO,
                    })
                ),
                (500_000, Some(own_path)),
            ]
        );
        // Nothing leaves, so the fee warning measures itself against the self-spend total
        // instead of dividing by a foreign value of zero.
        assert_eq!(prompt.foreign_value(), None);
        assert_eq!(prompt.value_at_risk(), prompt.value_moved());
    }
}
