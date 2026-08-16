//! Building a spend, as the two stages with different rights over the wallet: planning may only
//! read, committing is the single place a change address comes from. The boundary value
//! [`SendPlan`] is a finished coin selection that has reserved nothing, so a caller can plan (and
//! re-plan) as often as it likes — a fee display bound to a plan cannot move the wallet's
//! keychain indices.

use super::wallet::CoordSuperWallet;
use anyhow::{anyhow, Result};
use bdk_chain::{
    bitcoin::{self, Amount, OutPoint, TxOut},
    CanonicalizationParams,
};
use bdk_coin_select::{
    metrics, Candidate, ChangePolicy, CoinSelector, DrainWeights, FeeRate, Target, TargetFee,
    TargetOutputs, TR_DUST_RELAY_MIN_VALUE, TR_KEYSPEND_TXIN_WEIGHT,
};
use frostsnap_core::{
    bitcoin_transaction::{LocalSpk, PushInput, TransactionTemplate},
    tweak::{BitcoinAccountKeychain, BitcoinBip32Path},
    MasterAppkey,
};
use tracing::{event, Level};

/// A finished coin selection that has reserved nothing.
///
/// Each selected input is pinned as the keychain outpoint it came from — derivation path and
/// outpoint, its immutable identity; only its spendability can change. Every input is a taproot
/// keyspend of weight [`TR_KEYSPEND_TXIN_WEIGHT`], and the change output is a value without an
/// address. The plan carries the outputs it was selected FOR and the fee the selection fixed,
/// so the fee it reports is the fee the committed transaction pays. Only
/// [`CoordSuperWallet::commit_send`] turns it into something the wallet is committed to.
#[derive(Debug)]
pub struct SendPlan {
    master_appkey: MasterAppkey,
    selected: Vec<(BitcoinBip32Path, OutPoint)>,
    recipients: Vec<TxOut>,
    change_value: Option<u64>,
    fee: u64,
}

impl SendPlan {
    pub fn change_value(&self) -> Option<u64> {
        self.change_value
    }

    pub fn fee(&self) -> u64 {
        self.fee
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

        let utxos: Vec<(_, bdk_chain::FullTxOut<_>)> = self
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

        let selected = cs
            .apply_selection(&utxos)
            .map(|(((_, account_keychain), index), utxo)| {
                (
                    BitcoinBip32Path {
                        account_keychain: *account_keychain,
                        index: *index,
                    },
                    utxo.outpoint,
                )
            })
            .collect();

        let recipient_value: u64 = target_outputs.iter().map(|txo| txo.value.to_sat()).sum();
        let change_value = cs.drain_value(target, change_policy);
        Ok(SendPlan {
            master_appkey,
            selected,
            recipients: target_outputs,
            change_value,
            fee: cs
                .selected_value()
                .saturating_sub(recipient_value)
                .saturating_sub(change_value.unwrap_or(0)),
        })
    }

    /// Turn a [`SendPlan`] into a signable template. This is the wallet's single change-address
    /// allocation point.
    ///
    /// The plan pinned its inputs' immutable identities, so committing re-canonicalizes only
    /// the plan's own outpoints to re-check the one mutable fact: each must still be unspent —
    /// a plan can outlive a sync that spends one of its coins. The plan is dead then; the
    /// caller builds a new one.
    pub fn commit_send(&mut self, plan: &SendPlan) -> Result<TransactionTemplate> {
        self.lazily_initialize_key(plan.master_appkey);

        let still_unspent = self
            .tx_graph
            .graph()
            .filter_chain_unspents(
                self.chain.as_ref(),
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
                plan.selected.iter().copied(),
            )
            .count();
        if still_unspent != plan.selected.len() {
            return Err(anyhow!(
                "a planned input is no longer spendable ({still_unspent} of {} remain)",
                plan.selected.len()
            ));
        }

        let mut template = TransactionTemplate::new();

        for &(bip32_path, outpoint) in &plan.selected {
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
            let mut db = self.db.lock().unwrap();
            // No mark_used: it was RAM-only (the keychain changeset cannot carry it), so it died
            // on restart anyway while burning cancelled sends' indices within a session. Until a
            // committed tx reaches the graph, a subsequent send may pick the same change index —
            // reuse between our own in-flight txs, accepted as the rarer and safer failure.
            let (index, _change_spk) = self.tx_graph.mutate(&mut db, |tx_graph| {
                Ok(tx_graph
                    .index
                    .next_unused_spk(internal)
                    .expect("keychain initialized: we are spending from this wallet"))
            })?;

            template.push_owned_output(
                Amount::from_sat(value),
                LocalSpk {
                    master_appkey: plan.master_appkey,
                    bip32_path: BitcoinBip32Path {
                        account_keychain: BitcoinAccountKeychain::internal(),
                        index,
                    },
                },
            );
        }

        for txo in &plan.recipients {
            template.push_foreign_output(txo.clone());
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
    use frostsnap_core::schnorr_fun::fun::Point;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    const NETWORK: bitcoin::Network = bitcoin::Network::Bitcoin;

    /// The handler owns the receiving ends of the client's channels, so it must outlive every
    /// `ChainClient` call or `monitor_keychain`'s send panics.
    fn chain_client(db: &Arc<Mutex<rusqlite::Connection>>) -> (ChainClient, ConnectionHandler) {
        let trusted = {
            let mut conn = db.lock().unwrap();
            Persisted::new(&mut *conn, NETWORK).unwrap()
        };
        ChainClient::new(
            bitcoin::constants::genesis_block(NETWORK).block_hash(),
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
        _handler: ConnectionHandler,
        master_appkey: MasterAppkey,
        recipient: bitcoin::Address,
        blocks: Vec<BlockId>,
    }

    impl Fixture {
        fn new() -> Self {
            let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
            let (client, _handler) = chain_client(&db);
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
                _handler,
                master_appkey,
                recipient,
                blocks: vec![BlockId {
                    height: 0,
                    hash: bitcoin::constants::genesis_block(NETWORK).block_hash(),
                }],
            }
        }

        /// Deliver a confirmed external payment the way a sync would.
        fn fund(&mut self, index: u32, value: u64, height: u32) {
            let spk = crate::bitcoin::peek_spk(
                self.master_appkey,
                BitcoinBip32Path {
                    account_keychain: BitcoinAccountKeychain::external(),
                    index,
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
                    last_active_indices: [(
                        (self.master_appkey, BitcoinAccountKeychain::external()),
                        index,
                    )]
                    .into(),
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
        let template = f.wallet.commit_send(&plan).unwrap();

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
            let template = f.wallet.commit_send(&plan).unwrap();
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
            f.wallet.commit_send(&plan).unwrap();
            assert_eq!(f.last_revealed_internal(), Some(0));
        }
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

        f.wallet.commit_send(&plan).unwrap();
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
            .commit_send(&plan)
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
        // Past the load window, so only the persisted reveal can bring it back: inside the
        // window, indexing the hit would reveal it and the ratchet changeset would carry it.
        const FAR: u32 = 60;
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
        {
            let (client, _handler) = chain_client(&db);
            let mut wallet = CoordSuperWallet::load_or_init(db.clone(), NETWORK, client).unwrap();
            wallet.list_addresses(master_appkey);

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
                            index: FAR,
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
                    last_active_indices: [(external, FAR)].into(),
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
            Some(FAR),
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

        let err = f.wallet.commit_send(&plan).unwrap_err();
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
        f.wallet.commit_send(&fresh).unwrap();
        assert_eq!(f.last_revealed_internal(), Some(0));
    }
}
