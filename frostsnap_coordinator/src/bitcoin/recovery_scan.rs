//! Finds change that a gap-limit restore missed and makes it spendable again.
//!
//! An outgoing transaction is discoverable even when its change is not: a transaction is ours
//! if we own a spent prevout, so the change script of every outgoing transaction is already
//! sitting in the local graph as an unattributed txout. Recovering it is therefore a purely
//! local comparison against candidate scripts derived past the reveal frontier — the network
//! holds no information we lack.

use super::wallet::{CoordSuperWallet, KeychainId, WalletIndexedTxGraphChangeSet};
use anyhow::Result;
use bdk_chain::{
    bitcoin::{self, ScriptBuf, Txid},
    indexer::keychain_txout,
    CanonicalizationParams, Indexer, Merge, SpkIterator,
};
use frostsnap_core::{tweak::BitcoinAccountKeychain, MasterAppkey};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;
use tracing::{event, Level};

/// How many indices past the reveal frontier each keychain's candidates are derived. Applied per
/// keychain rather than shared across them: derivation is local and near-free, so the budget is
/// about coverage, not cost.
pub const RECOVERY_SCAN_STOP_GAP: u32 = 1000;

/// Scan progress, persisted apart from wallet derivation state.
///
/// Progress must never ride in `last_revealed`: persisting an unused frontier advances this
/// database's address allocation, manufacturing the very gap the scan exists to heal. This state
/// changes on every scan — matched or not — while derivation state changes only on a match.
///
/// Rows exist only while transactions are outstanding: `compared` records the candidate index
/// every outstanding transaction has been compared up to, so an unchanged wallet can skip the
/// derivation work entirely on the next scan.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecoveryScanState {
    pub(super) compared: BTreeMap<KeychainId, u32>,
    pub(super) outstanding: BTreeMap<MasterAppkey, BTreeSet<Txid>>,
}

impl RecoveryScanState {
    pub(super) fn apply_update(&mut self, update: &RecoveryScanUpdate) {
        self.compared
            .retain(|(key, _), _| *key != update.master_appkey);
        for (&account_keychain, &index) in &update.compared {
            self.compared
                .insert((update.master_appkey, account_keychain), index);
        }
        if update.outstanding.is_empty() {
            self.outstanding.remove(&update.master_appkey);
        } else {
            self.outstanding
                .insert(update.master_appkey, update.outstanding.clone());
        }
    }

    fn key_snapshot(&self, master_appkey: MasterAppkey) -> RecoveryScanUpdate {
        RecoveryScanUpdate {
            master_appkey,
            compared: self
                .compared
                .iter()
                .filter(|((key, _), _)| *key == master_appkey)
                .map(|((_, account_keychain), &index)| (*account_keychain, index))
                .collect(),
            outstanding: self
                .outstanding
                .get(&master_appkey)
                .cloned()
                .unwrap_or_default(),
        }
    }
}

/// Replaces one key's scan progress.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryScanUpdate {
    pub master_appkey: MasterAppkey,
    pub compared: BTreeMap<BitcoinAccountKeychain, u32>,
    pub outstanding: BTreeSet<Txid>,
}

/// What one [`CoordSuperWallet::recovery_scan`] call did and what it could not resolve.
#[derive(Debug, Clone, Default)]
pub struct RecoveryScanReport {
    /// Keychains a match advanced, with their new frontier.
    pub revealed: BTreeMap<BitcoinAccountKeychain, u32>,
    /// Outgoing transactions still missing an owned output after the budget — the honest
    /// "there may be funds we cannot see" set.
    pub outstanding: BTreeSet<Txid>,
    /// False when persisted progress already covered the wallet's current state and nothing was
    /// derived or compared.
    pub did_compare: bool,
}

impl CoordSuperWallet {
    /// Find change on outgoing transactions that a gap-limit restore left unattributed, reveal
    /// the matched indices and make the coins spendable.
    ///
    /// Detection, candidate derivation and comparison are all local; the network is never asked
    /// about candidates. On a match the reveal frontier advances exactly to the highest matched
    /// index and every stored transaction is re-indexed against the expanded script set.
    /// Iterates until every flagged transaction is reconciled or the budget is exhausted;
    /// whatever remains is reported as outstanding. Progress is durable, so a scan over an
    /// unchanged wallet does no work.
    pub fn recovery_scan(&mut self, master_appkey: MasterAppkey) -> Result<RecoveryScanReport> {
        self.lazily_initialize_key(master_appkey);
        let account_keychains = [
            BitcoinAccountKeychain::external(),
            BitcoinAccountKeychain::internal(),
        ];

        let mut report = RecoveryScanReport::default();

        let final_unreconciled = loop {
            let unreconciled = self.unreconciled_txs(master_appkey);
            if unreconciled.is_empty() {
                break unreconciled;
            }

            let ranges = account_keychains.map(|account_keychain| {
                let keychain_id = (master_appkey, account_keychain);
                let frontier = self.tx_graph.index.last_revealed_index(keychain_id);
                let start = frontier.map_or(0, |f| f + 1);
                let end = frontier.map_or(RECOVERY_SCAN_STOP_GAP - 1, |f| {
                    f.saturating_add(RECOVERY_SCAN_STOP_GAP)
                });
                (keychain_id, start..=end)
            });

            let covered = {
                let outstanding = self
                    .recovery_scan_state
                    .outstanding
                    .get(&master_appkey)
                    .cloned()
                    .unwrap_or_default();
                unreconciled.keys().all(|txid| outstanding.contains(txid))
                    && ranges.iter().all(|(keychain_id, range)| {
                        self.recovery_scan_state.compared.get(keychain_id).copied()
                            >= Some(*range.end())
                    })
            };
            if covered {
                break unreconciled;
            }
            report.did_compare = true;

            let target_spks: HashSet<&ScriptBuf> = unreconciled
                .values()
                .flat_map(|tx| tx.output.iter().map(|txout| &txout.script_pubkey))
                .collect();

            let mut matches = BTreeMap::<KeychainId, u32>::new();
            for (keychain_id, range) in &ranges {
                let descriptor = super::descriptor_for_account_keychain(
                    *keychain_id,
                    // spks don't depend on the network
                    bitcoin::NetworkKind::Main,
                );
                for (index, spk) in SpkIterator::new_with_range(descriptor, range.clone()) {
                    if target_spks.contains(&spk) {
                        // ascending, so the last hit is the highest
                        matches.insert(*keychain_id, index);
                    }
                }
            }

            if matches.is_empty() {
                let mut db = self.db.lock().unwrap();
                self.recovery_scan_state.mutate(&mut db, |state| {
                    let update = RecoveryScanUpdate {
                        master_appkey,
                        compared: ranges
                            .iter()
                            .map(|((_, account_keychain), range)| (*account_keychain, *range.end()))
                            .collect(),
                        outstanding: unreconciled.keys().copied().collect(),
                    };
                    state.apply_update(&update);
                    Ok(((), update))
                })?;
                break unreconciled;
            }

            event!(
                Level::INFO,
                master_appkey = master_appkey.to_redacted_string(),
                ?matches,
                "recovery scan matched stranded change"
            );

            {
                let mut db = self.db.lock().unwrap();
                self.tx_graph.mutate(&mut db, |tx_graph| {
                    let mut indexer_changeset = keychain_txout::ChangeSet::default();
                    for (&keychain_id, &target) in &matches {
                        let (_, changeset) = tx_graph
                            .index
                            .reveal_to_target(keychain_id, target)
                            .expect("keychain initialized above");
                        indexer_changeset.merge(changeset);
                    }
                    // bdk indexes a tx only against the spks derived at that moment, so a reveal
                    // strands the outputs of txs already in the graph unless every one is
                    // re-indexed — and any stored tx may pay a newly derived index, not just the
                    // tx that triggered the match.
                    let txs = tx_graph
                        .graph()
                        .full_txs()
                        .map(|tx_node| tx_node.tx.clone())
                        .collect::<Vec<_>>();
                    for tx in &txs {
                        indexer_changeset.merge(tx_graph.index.index_tx(tx));
                    }
                    Ok((
                        (),
                        WalletIndexedTxGraphChangeSet {
                            indexer: indexer_changeset,
                            ..Default::default()
                        },
                    ))
                })?;
            }

            for &keychain_id in matches.keys() {
                let frontier = self
                    .tx_graph
                    .index
                    .last_revealed_index(keychain_id)
                    .expect("just revealed");
                report.revealed.insert(keychain_id.1, frontier);
            }
            // matching extends reach: a newly indexed output can make an already-stored
            // descendant spend outgoing-and-unreconciled, so go around again
        };

        for (&account_keychain, &frontier) in &report.revealed {
            self.chain_client
                .monitor_keychain((master_appkey, account_keychain), frontier + 1);
        }

        report.outstanding = final_unreconciled.keys().copied().collect();

        let snapshot = RecoveryScanUpdate {
            master_appkey,
            compared: if final_unreconciled.is_empty() {
                BTreeMap::new()
            } else {
                self.recovery_scan_state
                    .key_snapshot(master_appkey)
                    .compared
            },
            outstanding: report.outstanding.clone(),
        };
        if snapshot != self.recovery_scan_state.key_snapshot(master_appkey) {
            let mut db = self.db.lock().unwrap();
            self.recovery_scan_state.mutate(&mut db, |state| {
                state.apply_update(&snapshot);
                Ok(((), snapshot))
            })?;
        }

        Ok(report)
    }

    /// Outgoing (a prevout is ours) with more than one output, none of them ours: the change has
    /// to be somewhere, and it isn't attributed to us. Single-output spends are deliberately not
    /// flagged — send-max legitimately has no change.
    fn unreconciled_txs(
        &self,
        master_appkey: MasterAppkey,
    ) -> BTreeMap<Txid, Arc<bitcoin::Transaction>> {
        self.tx_graph
            .graph()
            .list_ordered_canonical_txs(
                self.chain.as_ref(),
                self.chain.tip().block_id(),
                CanonicalizationParams::default(),
            )
            .filter_map(|canonical_tx| {
                let tx = &canonical_tx.tx_node.tx;
                if tx.output.len() < 2 {
                    return None;
                }
                let any_output_ours = tx.output.iter().any(|txout| {
                    self.spk_index(master_appkey, txout.script_pubkey.clone())
                        .is_some()
                });
                if any_output_ours {
                    return None;
                }
                let spends_ours = tx.input.iter().any(|txin| {
                    self.tx_graph
                        .graph()
                        .get_txout(txin.previous_output)
                        .is_some_and(|txout| {
                            self.spk_index(master_appkey, txout.script_pubkey.clone())
                                .is_some()
                        })
                });
                spends_ours.then(|| (canonical_tx.tx_node.txid, canonical_tx.tx_node.tx.clone()))
            })
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::bitcoin::test_util::{
        foreign_spk, fund_and_burn, last_revealed_rows, tx_paying, unspent_sum, Fixture, EXTERNAL,
        INTERNAL,
    };
    use bdk_chain::bitcoin::{hashes::Hash, OutPoint};

    #[test]
    fn no_match_mutates_nothing_but_records_progress() {
        let fixture = Fixture::new();
        let mut wallet = fixture.wallet();
        let (_, burn_txid) = fund_and_burn(&fixture, &mut wallet, None);

        let frontier_rows_before = last_revealed_rows(&fixture.db);
        let external = (fixture.master_appkey, EXTERNAL());
        let internal = (fixture.master_appkey, INTERNAL());
        assert_eq!(wallet.tx_graph.index.last_revealed_index(external), Some(0));
        assert_eq!(wallet.tx_graph.index.last_revealed_index(internal), None);

        let report = wallet.recovery_scan(fixture.master_appkey).unwrap();

        assert!(report.did_compare);
        assert!(report.revealed.is_empty());
        assert_eq!(report.outstanding, BTreeSet::from([burn_txid]));
        assert_eq!(wallet.tx_graph.index.last_revealed_index(external), Some(0));
        assert_eq!(wallet.tx_graph.index.last_revealed_index(internal), None);
        assert_eq!(last_revealed_rows(&fixture.db), frontier_rows_before);

        let state = fixture.reload_scan_state();
        assert_eq!(
            state.compared,
            BTreeMap::from([(external, 1000), (internal, 999)])
        );
        assert_eq!(
            state.outstanding,
            BTreeMap::from([(fixture.master_appkey, BTreeSet::from([burn_txid]))])
        );
    }

    #[test]
    fn match_reveals_to_matched_index_and_no_farther() {
        let fixture = Fixture::new();
        let mut wallet = fixture.wallet();
        let (_, _) = fund_and_burn(&fixture, &mut wallet, Some((800, 30_000)));

        assert_eq!(
            unspent_sum(&wallet, fixture.master_appkey),
            0,
            "the change is past the derived window, so nothing should be spendable yet"
        );

        let report = wallet.recovery_scan(fixture.master_appkey).unwrap();

        assert!(report.did_compare);
        assert!(report.outstanding.is_empty());
        assert_eq!(report.revealed, BTreeMap::from([(INTERNAL(), 800)]));
        assert_eq!(
            wallet
                .tx_graph
                .index
                .last_revealed_index((fixture.master_appkey, INTERNAL())),
            Some(800),
            "frontier must land exactly on the matched index, never the scan budget"
        );
        assert_eq!(
            wallet
                .tx_graph
                .index
                .last_revealed_index((fixture.master_appkey, EXTERNAL())),
            Some(0)
        );
        assert_eq!(unspent_sum(&wallet, fixture.master_appkey), 30_000);

        let state = fixture.reload_scan_state();
        assert_eq!(state, RecoveryScanState::default());
    }

    #[test]
    fn match_reindexes_every_stored_tx_not_just_the_trigger() {
        let fixture = Fixture::new();
        let mut wallet = fixture.wallet();

        let stray = tx_paying(
            vec![OutPoint {
                txid: Txid::from_byte_array([7u8; 32]),
                vout: 0,
            }],
            vec![(fixture.spk(INTERNAL(), 700), 40_000)],
        );
        let stray_txid = stray.compute_txid();
        fixture.deliver(&mut wallet, vec![stray], 99, None);

        fund_and_burn(&fixture, &mut wallet, Some((800, 30_000)));

        assert!(
            !wallet
                .list_transactions(fixture.master_appkey)
                .iter()
                .any(|tx| tx.txid == stray_txid),
            "the stray tx must be invisible before the scan"
        );

        wallet.recovery_scan(fixture.master_appkey).unwrap();

        assert_eq!(
            unspent_sum(&wallet, fixture.master_appkey),
            70_000,
            "the stray output below the new frontier must become spendable too"
        );
        assert!(wallet
            .list_transactions(fixture.master_appkey)
            .iter()
            .any(|tx| tx.txid == stray_txid));
    }

    #[test]
    fn restart_resumes_from_the_scan_table() {
        let fixture = Fixture::new();
        let mut wallet = fixture.wallet();
        let (_, burn_txid) = fund_and_burn(&fixture, &mut wallet, None);

        let first = wallet.recovery_scan(fixture.master_appkey).unwrap();
        assert!(first.did_compare);

        let again = wallet.recovery_scan(fixture.master_appkey).unwrap();
        assert!(!again.did_compare, "unchanged wallet, same session");
        assert_eq!(again.outstanding, BTreeSet::from([burn_txid]));

        drop(wallet);
        let mut wallet = fixture.wallet();
        let after_restart = wallet.recovery_scan(fixture.master_appkey).unwrap();
        assert!(
            !after_restart.did_compare,
            "progress must come back from the scan table, not be redone"
        );
        assert_eq!(after_restart.outstanding, BTreeSet::from([burn_txid]));
        assert_eq!(
            wallet
                .tx_graph
                .index
                .last_revealed_index((fixture.master_appkey, INTERNAL())),
            None,
            "scan progress must not ride in last_revealed"
        );
    }

    #[test]
    fn one_scan_reconciles_descendant_spends_already_in_the_graph() {
        let fixture = Fixture::new();
        let mut wallet = fixture.wallet();
        let (_, burn_txid) = fund_and_burn(&fixture, &mut wallet, Some((800, 30_000)));

        let descendant = tx_paying(
            vec![OutPoint {
                txid: burn_txid,
                vout: 1,
            }],
            vec![
                (foreign_spk(), 10_000),
                (fixture.spk(INTERNAL(), 900), 15_000),
            ],
        );
        fixture.deliver(&mut wallet, vec![descendant], 102, None);

        let report = wallet.recovery_scan(fixture.master_appkey).unwrap();

        assert!(report.outstanding.is_empty());
        assert_eq!(
            report.revealed,
            BTreeMap::from([(INTERNAL(), 900)]),
            "the reveal from the first match must surface the descendant's change in the same scan"
        );
        assert_eq!(unspent_sum(&wallet, fixture.master_appkey), 15_000);
    }

    #[test]
    fn descendant_arriving_by_sync_is_reconciled_by_the_next_scan() {
        let fixture = Fixture::new();
        let mut wallet = fixture.wallet();
        let (_, burn_txid) = fund_and_burn(&fixture, &mut wallet, Some((800, 30_000)));

        let first = wallet.recovery_scan(fixture.master_appkey).unwrap();
        assert_eq!(first.revealed, BTreeMap::from([(INTERNAL(), 800)]));

        // ordinary monitoring of the now-revealed script delivers the spend later
        let descendant = tx_paying(
            vec![OutPoint {
                txid: burn_txid,
                vout: 1,
            }],
            vec![
                (foreign_spk(), 10_000),
                (fixture.spk(INTERNAL(), 900), 15_000),
            ],
        );
        fixture.deliver(&mut wallet, vec![descendant], 102, None);

        let second = wallet.recovery_scan(fixture.master_appkey).unwrap();
        assert_eq!(second.revealed, BTreeMap::from([(INTERNAL(), 900)]));
        assert!(second.outstanding.is_empty());
        assert_eq!(unspent_sum(&wallet, fixture.master_appkey), 15_000);
    }
}
