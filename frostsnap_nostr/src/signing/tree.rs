//! Per-request buffer for signing events, with settling-timer based round
//! confirmation.
//!
//! Offers form a **flat set** under a `Request`: every offer's reply_to is
//! the Request, and the tree keeps all observed offers in
//! `RequestState.offers`. A settling timer (driven from
//! [`crate::signing::ChannelClient`]) fires after a quiet period following
//! the last offer arrival; on expiry, the tree deterministically selects a
//! subset of size `threshold` ordered by `(timestamp, event_id)`, computes
//! `session_id` via `GroupSignReq::from_binonces`, and emits either
//! [`SigningEvent::RoundConfirmed`] (locking in the subset) or, if
//! threshold hasn't been reached, [`SigningEvent::RoundPending`] (a
//! provisional snapshot that may fire multiple times as offers trickle
//! in). The only round-termination paths are `RoundConfirmed` (success)
//! and an explicit `SigningEvent::Cancel` from the requester.
//!
//! The tree owns no time source itself — it returns [`TimerAction`] values
//! alongside [`SigningEvent`]s that the outer event loop translates into
//! `tokio::time` state.
//!
//! Partials and Cancels keep their DAG-orphan buffering semantics from the
//! prior chain-based design: if a partial/cancel arrives before its
//! request, it's stashed and replayed when the request lands.

use super::events::{ConfirmedSubsetEntry, SigningEvent};
use crate::channel_runner::EventMeta;
use crate::{EventId, PublicKey};
use frostsnap_core::{
    coordinator::{KeyContext, ParticipantBinonces, ParticipantSignatureShares},
    message::GroupSignReq,
    schnorr_fun::frost::ShareIndex,
    WireSignTask,
};
use std::collections::{BTreeSet, HashMap, VecDeque};

pub(crate) struct SigningEventTree {
    requests: HashMap<EventId, RequestState>,
    /// Wire events buffered because a required ancestor (Request or referenced
    /// Offer) hasn't landed yet. Keyed by the missing event id. Replayed
    /// through `ingest_wire` when the ancestor arrives.
    pending: HashMap<EventId, Vec<(EventMeta, WireEvent)>>,
    key_context: KeyContext,
    settling_window: std::time::Duration,
}

struct RequestState {
    author: PublicKey,
    sign_task: WireSignTask,
    /// Stored `ConfirmedSubsetEntry` because that's exactly what the
    /// `RoundConfirmed` emission needs — {event_id, author, timestamp,
    /// binonces}. Storing it in final shape saves a conversion.
    offers: HashMap<EventId, ConfirmedSubsetEntry>,
    /// First-writer-wins dedup: an offer carrying a share_index already in
    /// this set is rejected without mutating state.
    seen_share_indices: BTreeSet<ShareIndex>,
    /// Set once the settling timer expires with >= threshold offers and
    /// `RoundConfirmed` has been emitted. Further timer fires on a
    /// confirmed round are no-ops.
    confirmed: bool,
    /// Set when the requester publishes Cancel. Further offers are rejected;
    /// timer fires are no-ops.
    cancelled: bool,
}

/// Decoded wire event after the caller has resolved the e-tag. Mirrors
/// [`crate::signing::SigningMessage`] but with the referenced request_id
/// promoted out of the e-tag for the variants that need it. The outer
/// nostr-event metadata travels alongside in `EventMeta`, not on the enum.
#[derive(Debug, Clone)]
pub(crate) enum WireEvent {
    Request {
        sign_task: WireSignTask,
        message: String,
    },
    Offer {
        request_id: EventId,
        binonces: Vec<ParticipantBinonces>,
    },
    Partial {
        request_id: EventId,
        /// Offer event ids this partial signed against. The tree resolves
        /// each to its `ConfirmedSubsetEntry` to recover binonces, then
        /// derives `session_id` from them before emitting
        /// [`SigningEvent::Partial`]. Stashed until every referenced offer
        /// has landed.
        offer_subset: Vec<EventId>,
        signature_shares: ParticipantSignatureShares,
    },
    Cancel {
        request_id: EventId,
    },
}

/// Timer side-effect the caller must apply after `ingest_wire` returns.
#[derive(Debug, Clone)]
pub(crate) enum TimerAction {
    /// (Re)start the settling timer for this request.
    Set {
        request_id: EventId,
        duration: std::time::Duration,
    },
    /// Cancel the settling timer for this request.
    Cancel { request_id: EventId },
}

impl SigningEventTree {
    pub(crate) fn new(key_context: KeyContext, settling_window: std::time::Duration) -> Self {
        Self {
            requests: HashMap::new(),
            pending: HashMap::new(),
            key_context,
            settling_window,
        }
    }

    fn threshold(&self) -> usize {
        self.key_context.threshold()
    }

    /// Ingest one decoded wire event (with its outer-nostr metadata) and
    /// return the signing events + timer actions it unlocks, in DAG-valid
    /// order (ancestor before descendant).
    ///
    /// When the event is stashed (missing ancestor), both Vecs are empty.
    /// When it's processed, the first `SigningEvent` corresponds to this
    /// event. Orphan cascades may append further events from previously
    /// stashed items.
    pub(crate) fn ingest_wire(
        &mut self,
        meta: EventMeta,
        wire: WireEvent,
    ) -> (Vec<SigningEvent>, Vec<TimerAction>) {
        let mut events = Vec::new();
        let mut timers = Vec::new();
        let mut queue: VecDeque<(EventMeta, WireEvent)> = VecDeque::new();
        queue.push_back((meta, wire));

        while let Some((meta, wire)) = queue.pop_front() {
            match wire {
                WireEvent::Request { sign_task, message } => {
                    self.requests.insert(
                        meta.event_id,
                        RequestState {
                            author: meta.author,
                            sign_task: sign_task.clone(),
                            offers: HashMap::new(),
                            seen_share_indices: BTreeSet::new(),
                            confirmed: false,
                            cancelled: false,
                        },
                    );
                    events.push(SigningEvent::Request {
                        event_id: meta.event_id,
                        author: meta.author,
                        sign_task,
                        message,
                        timestamp: meta.timestamp,
                    });
                    self.drain_pending_into(meta.event_id, &mut queue);
                }
                WireEvent::Offer {
                    request_id,
                    binonces,
                } => {
                    let Some(state) = self.requests.get_mut(&request_id) else {
                        tracing::debug!(
                            event_id = %meta.event_id,
                            request_id = %request_id,
                            "signing offer stashed pending request",
                        );
                        self.pending.entry(request_id).or_default().push((
                            meta,
                            WireEvent::Offer {
                                request_id,
                                binonces,
                            },
                        ));
                        continue;
                    };

                    if state.confirmed || state.cancelled {
                        events.push(SigningEvent::Rejected {
                            event_id: meta.event_id,
                            author: meta.author,
                            timestamp: meta.timestamp,
                            reason: "offer arrived after round was decided".into(),
                        });
                        continue;
                    }

                    let new_share_indices: BTreeSet<ShareIndex> =
                        binonces.iter().map(|b| b.share_index).collect();
                    let collision = !state.seen_share_indices.is_disjoint(&new_share_indices);
                    if collision {
                        events.push(SigningEvent::Rejected {
                            event_id: meta.event_id,
                            author: meta.author,
                            timestamp: meta.timestamp,
                            reason: "offer contains a share_index already seen this round".into(),
                        });
                        continue;
                    }

                    state.seen_share_indices.extend(new_share_indices);
                    state.offers.insert(
                        meta.event_id,
                        ConfirmedSubsetEntry {
                            event_id: meta.event_id,
                            author: meta.author,
                            timestamp: meta.timestamp,
                            binonces: binonces.clone(),
                        },
                    );

                    timers.push(TimerAction::Set {
                        request_id,
                        duration: self.settling_window,
                    });
                    events.push(SigningEvent::Offer {
                        event_id: meta.event_id,
                        author: meta.author,
                        request_id,
                        binonces,
                        timestamp: meta.timestamp,
                    });
                    self.drain_pending_into(meta.event_id, &mut queue);
                }
                WireEvent::Partial {
                    request_id,
                    offer_subset,
                    signature_shares,
                } => {
                    let Some(state) = self.requests.get(&request_id) else {
                        tracing::debug!(
                            event_id = %meta.event_id,
                            request_id = %request_id,
                            "signing partial stashed pending request",
                        );
                        self.pending.entry(request_id).or_default().push((
                            meta,
                            WireEvent::Partial {
                                request_id,
                                offer_subset,
                                signature_shares,
                            },
                        ));
                        continue;
                    };

                    if offer_subset.is_empty() {
                        events.push(SigningEvent::Rejected {
                            event_id: meta.event_id,
                            author: meta.author,
                            timestamp: meta.timestamp,
                            reason: "partial has empty offer_subset".into(),
                        });
                        continue;
                    }
                    let dedup: BTreeSet<EventId> = offer_subset.iter().copied().collect();
                    if dedup.len() != offer_subset.len() {
                        events.push(SigningEvent::Rejected {
                            event_id: meta.event_id,
                            author: meta.author,
                            timestamp: meta.timestamp,
                            reason: "partial offer_subset has duplicates".into(),
                        });
                        continue;
                    }

                    let first_missing = offer_subset
                        .iter()
                        .find(|oid| !state.offers.contains_key(oid))
                        .copied();
                    if let Some(missing) = first_missing {
                        tracing::debug!(
                            event_id = %meta.event_id,
                            request_id = %request_id,
                            missing_offer = %missing,
                            "signing partial stashed pending offer",
                        );
                        self.pending.entry(missing).or_default().push((
                            meta,
                            WireEvent::Partial {
                                request_id,
                                offer_subset,
                                signature_shares,
                            },
                        ));
                        continue;
                    }
                    let resolved_binonces: Vec<ParticipantBinonces> = offer_subset
                        .iter()
                        .flat_map(|oid| state.offers[oid].binonces.iter().cloned())
                        .collect();

                    let session_id = GroupSignReq::from_binonces(
                        state.sign_task.clone(),
                        self.key_context.access_structure_id(),
                        &resolved_binonces,
                    )
                    .session_id();

                    events.push(SigningEvent::Partial {
                        event_id: meta.event_id,
                        author: meta.author,
                        request_id,
                        offer_subset,
                        session_id,
                        signature_shares,
                        timestamp: meta.timestamp,
                    });
                }
                WireEvent::Cancel { request_id } => match self.requests.get_mut(&request_id) {
                    Some(state) => {
                        if state.author != meta.author {
                            events.push(SigningEvent::Rejected {
                                event_id: meta.event_id,
                                author: meta.author,
                                timestamp: meta.timestamp,
                                reason: "cancel from non-request-author".into(),
                            });
                            continue;
                        }
                        let still_collecting = !state.confirmed && !state.cancelled;
                        if still_collecting {
                            state.cancelled = true;
                            timers.push(TimerAction::Cancel { request_id });
                        }
                        events.push(SigningEvent::Cancel {
                            event_id: meta.event_id,
                            author: meta.author,
                            request_id,
                            timestamp: meta.timestamp,
                        });
                    }
                    None => {
                        tracing::debug!(
                            event_id = %meta.event_id,
                            request_id = %request_id,
                            "signing cancel stashed pending request",
                        );
                        self.pending
                            .entry(request_id)
                            .or_default()
                            .push((meta, WireEvent::Cancel { request_id }));
                    }
                },
            }
        }
        (events, timers)
    }

    pub(crate) fn timer_expired(&mut self, request_id: EventId) -> Option<SigningEvent> {
        let threshold = self.threshold();
        let access_structure_id = self.key_context.access_structure_id();
        let state = self.requests.get_mut(&request_id)?;
        if state.confirmed || state.cancelled {
            return None;
        }

        let observed_count = state.offers.len();
        if observed_count < threshold {
            // Not enough offers yet — the round is still collecting. Emit
            // a provisional snapshot so the UI can say "your offer is
            // likely accepted; waiting for more devices." The selector is
            // oldest-first, so any offer already in this set stays in the
            // final confirmed subset unless an offer with an earlier
            // timestamp arrives later. Do not change state.
            let observed: Vec<EventId> = state.offers.keys().copied().collect();
            return Some(SigningEvent::RoundPending {
                request_id,
                observed,
                threshold,
                timestamp: now_seconds(),
            });
        }

        let subset: Vec<ConfirmedSubsetEntry> = select_signing_subset(&state.offers, threshold)
            .into_iter()
            .cloned()
            .collect();
        let selected_binonces: Vec<ParticipantBinonces> = subset
            .iter()
            .flat_map(|rec| rec.binonces.iter().cloned())
            .collect();
        let session_id = GroupSignReq::from_binonces(
            state.sign_task.clone(),
            access_structure_id,
            &selected_binonces,
        )
        .session_id();

        state.confirmed = true;
        Some(SigningEvent::RoundConfirmed {
            request_id,
            subset,
            session_id,
            sign_task: state.sign_task.clone(),
            timestamp: now_seconds(),
        })
    }

    fn drain_pending_into(
        &mut self,
        landed: EventId,
        queue: &mut VecDeque<(EventMeta, WireEvent)>,
    ) {
        if let Some(drained) = self.pending.remove(&landed) {
            tracing::debug!(
                landed = %landed,
                count = drained.len(),
                "releasing stashed signing events",
            );
            queue.extend(drained);
        }
    }
}

/// Deterministic subset selection: sort by `(timestamp, event_id)` ascending
/// and take the first `threshold`.
fn select_signing_subset(
    offers: &HashMap<EventId, ConfirmedSubsetEntry>,
    threshold: usize,
) -> Vec<&ConfirmedSubsetEntry> {
    let mut sorted: Vec<&ConfirmedSubsetEntry> = offers.values().collect();
    sorted.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    sorted.truncate(threshold);
    sorted
}

/// Wall-clock timestamp for locally-generated events (`RoundConfirmed`,
/// `RoundPending`). Uses `SystemTime`, not `tokio::time`, so it is *not*
/// affected by `tokio::time::pause()` in tests. This is fine because these
/// timestamps are purely informational (for UI display), not used in any
/// consensus or ordering decision.
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use frostsnap_core::device::KeyPurpose;
    use frostsnap_core::schnorr_fun::{
        binonce,
        frost::SharedKey,
        fun::{g, marker::*, Point, Scalar, G},
    };
    use frostsnap_core::tweak::Xpub;
    use nostr_sdk::Keys;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    fn test_key_context(threshold: u16) -> KeyContext {
        // Build a toy SharedKey with the requested threshold via a fixed-seed
        // random polynomial. We only need access_structure_id() + threshold()
        // to work for the tree; signature verification isn't exercised here.
        let mut rng = ChaCha20Rng::seed_from_u64(0xA55_u64 ^ threshold as u64);
        let poly: Vec<Point<Normal, Public, Zero>> = (0..threshold)
            .map(|_| {
                let scalar = Scalar::<Secret, NonZero>::random(&mut rng);
                g!(scalar * G).normalize().mark_zero()
            })
            .collect();
        let shared_key = SharedKey::from_poly(poly).non_zero().unwrap();
        let xpub = Xpub::from_rootkey(shared_key).rootkey_to_master_appkey();
        KeyContext {
            app_shared_key: xpub,
            purpose: KeyPurpose::Test,
        }
    }

    fn eid(n: u8) -> EventId {
        let mut bytes = [0u8; 32];
        bytes[0] = n;
        EventId(bytes)
    }

    fn pk() -> PublicKey {
        Keys::generate().public_key().into()
    }

    fn test_task() -> WireSignTask {
        WireSignTask::Test {
            message: "test".to_string(),
        }
    }

    fn share_index(n: u32) -> ShareIndex {
        Scalar::<Secret, NonZero>::from(core::num::NonZeroU32::new(n).unwrap()).public()
    }

    fn meta(event_id: EventId, author: PublicKey, ts: u64) -> EventMeta {
        EventMeta {
            event_id,
            author,
            timestamp: ts,
        }
    }

    fn req(event_id: EventId, author: PublicKey, ts: u64) -> (EventMeta, WireEvent) {
        (
            meta(event_id, author, ts),
            WireEvent::Request {
                sign_task: test_task(),
                message: "r".into(),
            },
        )
    }

    fn offer_with(
        event_id: EventId,
        request_id: EventId,
        author: PublicKey,
        ts: u64,
        share_idx: u32,
    ) -> (EventMeta, WireEvent) {
        (
            meta(event_id, author, ts),
            WireEvent::Offer {
                request_id,
                binonces: vec![ParticipantBinonces {
                    share_index: share_index(share_idx),
                    binonces: vec![binonce::Nonce([Point::generator(), Point::generator()])],
                }],
            },
        )
    }

    fn partial_for(
        event_id: EventId,
        request_id: EventId,
        author: PublicKey,
        ts: u64,
        offer_subset: Vec<EventId>,
    ) -> (EventMeta, WireEvent) {
        (
            meta(event_id, author, ts),
            WireEvent::Partial {
                request_id,
                offer_subset,
                signature_shares: ParticipantSignatureShares {
                    share_index: share_index(1),
                    signature_shares: vec![],
                },
            },
        )
    }

    fn cancel(
        event_id: EventId,
        request_id: EventId,
        author: PublicKey,
        ts: u64,
    ) -> (EventMeta, WireEvent) {
        (meta(event_id, author, ts), WireEvent::Cancel { request_id })
    }

    const TEST_SETTLING: std::time::Duration = std::time::Duration::from_secs(4);

    fn ingest_tuple(
        tree: &mut SigningEventTree,
        pair: (EventMeta, WireEvent),
    ) -> (Vec<SigningEvent>, Vec<TimerAction>) {
        tree.ingest_wire(pair.0, pair.1)
    }

    fn event_ids(events: &[SigningEvent]) -> Vec<(EventId, &'static str)> {
        events
            .iter()
            .map(|e| match e {
                SigningEvent::Request { event_id, .. } => (*event_id, "Request"),
                SigningEvent::Offer { event_id, .. } => (*event_id, "Offer"),
                SigningEvent::Partial { event_id, .. } => (*event_id, "Partial"),
                SigningEvent::Cancel { event_id, .. } => (*event_id, "Cancel"),
                SigningEvent::RoundConfirmed { request_id, .. } => (*request_id, "RoundConfirmed"),
                SigningEvent::RoundPending { request_id, .. } => (*request_id, "RoundPending"),
                SigningEvent::Rejected { event_id, .. } => (*event_id, "Rejected"),
            })
            .collect()
    }

    fn timer_ids(actions: &[TimerAction]) -> Vec<(EventId, &'static str)> {
        actions
            .iter()
            .map(|a| match a {
                TimerAction::Set { request_id, .. } => (*request_id, "Set"),
                TimerAction::Cancel { request_id } => (*request_id, "Cancel"),
            })
            .collect()
    }

    #[test]
    fn first_offer_emits_start_timer_and_ready() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let o = eid(2);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let out = ingest_tuple(&mut tree, offer_with(o, r, pk(), 200, 1));
        assert_eq!(timer_ids(&out.1), vec![(r, "Set")]);
        assert_eq!(event_ids(&out.0), vec![(o, "Offer")]);
    }

    #[test]
    fn subsequent_offers_also_reset_timer() {
        let mut tree = SigningEventTree::new(test_key_context(3), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        let out = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 210, 2));
        assert_eq!(timer_ids(&out.1), vec![(r, "Set")]);
    }

    #[test]
    fn timer_expiry_below_threshold_emits_pending_without_decision() {
        let mut tree = SigningEventTree::new(test_key_context(3), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        match tree.timer_expired(r).unwrap() {
            SigningEvent::RoundPending {
                observed,
                threshold,
                ..
            } => {
                assert_eq!(observed.len(), 1);
                assert_eq!(observed[0], eid(2));
                assert_eq!(threshold, 3);
            }
            other => panic!("expected RoundPending, got {other:?}"),
        }
        // The round is not decided; a later offer can still arrive and
        // confirm normally.
        let _ = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 210, 2));
        let _ = ingest_tuple(&mut tree, offer_with(eid(4), r, pk(), 220, 3));
        match tree.timer_expired(r).unwrap() {
            SigningEvent::RoundConfirmed { subset, .. } => {
                assert_eq!(subset.len(), 3);
            }
            other => panic!("expected RoundConfirmed, got {other:?}"),
        }
    }

    #[test]
    fn timer_expiry_at_threshold_confirms_with_all_offers() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        let _ = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 210, 2));
        match tree.timer_expired(r).unwrap() {
            SigningEvent::RoundConfirmed { subset, .. } => {
                assert_eq!(subset.len(), 2);
                assert_eq!(subset[0].event_id, eid(2));
                assert_eq!(subset[1].event_id, eid(3));
            }
            other => panic!("expected RoundConfirmed, got {other:?}"),
        }
    }

    #[test]
    fn timer_expiry_above_threshold_selects_lowest_timestamp_first() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        // Oldest two offers win: eid(3)@150 and eid(5)@160; eid(4)@200 drops.
        let _ = ingest_tuple(&mut tree, offer_with(eid(4), r, pk(), 200, 1));
        let _ = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 150, 2));
        let _ = ingest_tuple(&mut tree, offer_with(eid(5), r, pk(), 160, 3));
        match tree.timer_expired(r).unwrap() {
            SigningEvent::RoundConfirmed { subset, .. } => {
                assert_eq!(subset.len(), 2);
                assert_eq!(subset[0].event_id, eid(3));
                assert_eq!(subset[1].event_id, eid(5));
            }
            other => panic!("expected RoundConfirmed, got {other:?}"),
        }
    }

    #[test]
    fn timer_expiry_ties_on_timestamp_broken_by_event_id() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(5), r, pk(), 200, 1));
        let _ = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 200, 2));
        let _ = ingest_tuple(&mut tree, offer_with(eid(4), r, pk(), 200, 3));
        match tree.timer_expired(r).unwrap() {
            SigningEvent::RoundConfirmed { subset, .. } => {
                assert_eq!(subset[0].event_id, eid(3));
                assert_eq!(subset[1].event_id, eid(4));
            }
            other => panic!("expected RoundConfirmed, got {other:?}"),
        }
    }

    #[test]
    fn offer_after_round_decided_rejects() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        let _ = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 210, 2));
        let _ = tree.timer_expired(r);
        let out = ingest_tuple(&mut tree, offer_with(eid(4), r, pk(), 220, 3));
        assert_eq!(event_ids(&out.0), vec![(eid(4), "Rejected")]);
    }

    #[test]
    fn duplicate_share_index_offer_rejects_without_mutating_state() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        let out = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 210, 1)); // same share_index
        assert_eq!(event_ids(&out.0), vec![(eid(3), "Rejected")]);
        // Second, non-colliding offer still goes through.
        let out2 = ingest_tuple(&mut tree, offer_with(eid(4), r, pk(), 220, 2));
        assert_eq!(event_ids(&out2.0), vec![(eid(4), "Offer")]);
    }

    #[test]
    fn offer_before_request_stashes_and_releases_on_request() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let o = eid(2);
        let out1 = ingest_tuple(&mut tree, offer_with(o, r, pk(), 200, 1));
        assert!(out1.0.is_empty());
        let out2 = ingest_tuple(&mut tree, req(r, pk(), 100));
        assert_eq!(event_ids(&out2.0), vec![(r, "Request"), (o, "Offer")]);
        // The released offer also produces a TimerAction::Set.
        assert_eq!(timer_ids(&out2.1), vec![(r, "Set")]);
    }

    #[test]
    fn cancel_during_collecting_emits_cancel_timer_and_cancel_event() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let author = pk();
        let r = eid(1);
        let c = eid(9);
        let _ = ingest_tuple(&mut tree, req(r, author, 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        let out = ingest_tuple(&mut tree, cancel(c, r, author, 250));
        assert_eq!(timer_ids(&out.1), vec![(r, "Cancel")]);
        assert_eq!(event_ids(&out.0), vec![(c, "Cancel")]);
    }

    #[test]
    fn cancel_from_non_author_rejects() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let author = pk();
        let imposter = pk();
        let r = eid(1);
        let c = eid(9);
        let _ = ingest_tuple(&mut tree, req(r, author, 100));
        let out = ingest_tuple(&mut tree, cancel(c, r, imposter, 250));
        assert_eq!(event_ids(&out.0), vec![(c, "Rejected")]);
        assert_eq!(timer_ids(&out.1), Vec::<(EventId, &str)>::new());
    }

    #[test]
    fn partial_stashes_pending_offer_and_releases_when_offer_lands() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let o1 = eid(2);
        let o2 = eid(3);
        let p = eid(9);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(o1, r, pk(), 200, 1));

        // Partial references both offers but o2 hasn't landed yet — stashed.
        let out1 = ingest_tuple(&mut tree, partial_for(p, r, pk(), 300, vec![o1, o2]));
        assert!(
            event_ids(&out1.0).is_empty(),
            "partial must not emit until all referenced offers land",
        );

        // Landing o2 re-examines and the partial fires.
        let out2 = ingest_tuple(&mut tree, offer_with(o2, r, pk(), 210, 2));
        let ready = event_ids(&out2.0);
        assert_eq!(ready, vec![(o2, "Offer"), (p, "Partial")]);
    }

    #[test]
    fn partial_before_request_is_stashed_and_released_when_request_and_offers_land() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let o1 = eid(2);
        let o2 = eid(3);
        let p = eid(9);
        // Partial arrives first — no request yet.
        let out0 = ingest_tuple(&mut tree, partial_for(p, r, pk(), 50, vec![o1, o2]));
        assert!(out0.0.is_empty());
        // Request lands: partial re-enqueued but offers still missing.
        let out1 = ingest_tuple(&mut tree, req(r, pk(), 100));
        assert_eq!(event_ids(&out1.0), vec![(r, "Request")]);
        // o1 lands: partial re-examined, still pending on o2.
        let out2 = ingest_tuple(&mut tree, offer_with(o1, r, pk(), 110, 1));
        assert_eq!(event_ids(&out2.0), vec![(o1, "Offer")]);
        // o2 lands: partial fires.
        let out3 = ingest_tuple(&mut tree, offer_with(o2, r, pk(), 120, 2));
        assert_eq!(event_ids(&out3.0), vec![(o2, "Offer"), (p, "Partial")]);
    }

    #[test]
    fn partial_with_duplicate_offer_subset_rejects() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let o1 = eid(2);
        let p = eid(9);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(o1, r, pk(), 200, 1));
        let out = ingest_tuple(&mut tree, partial_for(p, r, pk(), 300, vec![o1, o1]));
        assert_eq!(event_ids(&out.0), vec![(p, "Rejected")]);
    }

    #[test]
    fn independent_requests_resolve_independently() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let ra = eid(1);
        let rb = eid(2);
        let _ = ingest_tuple(&mut tree, req(ra, pk(), 100));
        let _ = ingest_tuple(&mut tree, req(rb, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(3), ra, pk(), 110, 1));
        let _ = ingest_tuple(&mut tree, offer_with(eid(4), ra, pk(), 120, 2));
        assert!(matches!(
            tree.timer_expired(ra).unwrap(),
            SigningEvent::RoundConfirmed { .. }
        ));
        // rb never got any offers; below-threshold timer fires emit
        // RoundPending with an empty observed set, not a decision.
        assert!(matches!(
            tree.timer_expired(rb).unwrap(),
            SigningEvent::RoundPending { .. }
        ));
    }

    #[test]
    fn timer_expired_after_cancel_is_noop() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let author = pk();
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, author, 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        let _ = ingest_tuple(&mut tree, cancel(eid(9), r, author, 250));
        // Cancel is the only round-termination signal; the timer firing
        // after cancel should not emit anything.
        assert!(tree.timer_expired(r).is_none());
    }

    #[test]
    fn redundant_timer_expired_after_decision_is_noop() {
        let mut tree = SigningEventTree::new(test_key_context(2), TEST_SETTLING);
        let r = eid(1);
        let _ = ingest_tuple(&mut tree, req(r, pk(), 100));
        let _ = ingest_tuple(&mut tree, offer_with(eid(2), r, pk(), 200, 1));
        let _ = ingest_tuple(&mut tree, offer_with(eid(3), r, pk(), 210, 2));
        let _ = tree.timer_expired(r);
        assert!(tree.timer_expired(r).is_none());
    }
}
