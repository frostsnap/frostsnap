mod events;
mod tree;

pub use events::{
    ChannelEvent, ChannelParticipant, ConfirmedSubsetEntry, ConnectionState, GroupMember,
    ObservationKind, SigningEvent,
};
use events::{ReceiveAddressPayload, SigningMessage};

use self::tree::{SigningEventTree, TimerAction, WireEvent};
use crate::{EventId, PublicKey};
use crate::{
    channel::ChannelKeys,
    channel_runner::{
        decode_bincode, extract_e_tag, ChannelMessageDraft, ChannelRunner, ChannelRunnerEvent,
        ChannelRunnerHandle, EventMeta, NostrProfile,
    },
};
use anyhow::{anyhow, Result};
use frostsnap_coordinator::Sink;
use frostsnap_core::{
    coordinator::{KeyContext, ParticipantBinonces, ParticipantSignatureShares},
    WireSignTask,
};
use nostr_sdk::{Client, Event, Keys, Kind};
use std::collections::{BTreeMap, HashMap};
use std::future::{self, Future};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep_until, Instant};

const KIND_FROSTSNAP_SIGNING: Kind = Kind::Custom(9001);
const KIND_FROSTSNAP_RECEIVE_ADDRESS: Kind = Kind::Custom(7800);

/// How long to wait for more sign offers before deciding the round. Every
/// new offer resets the timer; when this window elapses without a new offer
/// landing, the tree emits [`SigningEvent::RoundConfirmed`] or
/// [`SigningEvent::RoundAborted`].
pub const DEFAULT_SETTLING_WINDOW: Duration = Duration::from_millis(4000);

// ============================================================================
// ChannelClient
// ============================================================================

pub struct ChannelClient {
    channel_keys: ChannelKeys,
    key_context: KeyContext,
    init_data: Option<crate::ChannelInitData>,
    settling_window: Duration,
}

impl ChannelClient {
    pub fn new(key_context: KeyContext, init_data: Option<crate::ChannelInitData>) -> Self {
        let channel_keys =
            ChannelKeys::from_access_structure_id(&key_context.access_structure_id());
        Self {
            channel_keys,
            key_context,
            init_data,
            settling_window: DEFAULT_SETTLING_WINDOW,
        }
    }

    /// Override the settling window (how long to wait for more offers before
    /// deciding the round). Tests use a small window together with
    /// `tokio::time::pause()`.
    pub fn with_settling_window(mut self, d: Duration) -> Self {
        self.settling_window = d;
        self
    }

    pub async fn run(
        self,
        client: Client,
        sink: impl Sink<ChannelEvent> + Clone,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Result<ChannelHandle> {
        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connecting));

        // Build channel init inner event if we have init data to publish
        let mut runner = ChannelRunner::new(self.channel_keys.clone());
        if let Some(init_data) = &self.init_data {
            let init_inner = init_data.to_channel_creation_event()?;
            runner = runner.with_init_event(init_inner);
        }

        let (runner_handle, mut events) = runner.run(client, shutdown_rx).await?;

        // Chat keeps the existing cmd_tx flow (optimistic ChatMessage
        // emit + `publish_prepared` + `MessageSent`/`MessageSendFailed`
        // on the sink). Signing events bypass the cmd loop entirely
        // and call `runner_handle.dispatch` directly — the AppEvent
        // branch below then handles the resulting echo identically
        // to a peer event (Sink fires with `pending: false`).
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ChannelCommand>(32);

        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connected));

        let runner_handle_for_task = runner_handle.clone();
        let key_context = self.key_context.clone();
        let settling_window = self.settling_window;
        tokio::spawn(async move {
            let mut tree = SigningEventTree::new(key_context, settling_window);
            let mut timers: HashMap<EventId, Instant> = HashMap::new();
            let mut activity = ActivityState::default();

            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(ChannelCommand::SendPreparedMessage(prepared)) => {
                                let message_id: EventId = prepared.id.into();
                                sink.send(ChannelEvent::from_inner_chat_message(
                                    &prepared, true,
                                ));
                                match runner_handle_for_task.publish_prepared(prepared).await {
                                    Ok(outcome) if outcome.any_relay_success() => {
                                        sink.send(ChannelEvent::MessageSent { message_id });
                                    }
                                    Ok(outcome) => {
                                        let reason = format!(
                                            "no relay accepted: {:?}",
                                            outcome.relay_failed
                                        );
                                        tracing::error!(%reason, "no relay accepted chat message");
                                        sink.send(ChannelEvent::MessageSendFailed {
                                            message_id, reason,
                                        });
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "failed to send chat message");
                                        sink.send(ChannelEvent::MessageSendFailed {
                                            message_id, reason: e.to_string(),
                                        });
                                    }
                                }
                            }
                            Some(ChannelCommand::SendPreparedReceiveAddress(prepared, payload)) => {
                                let message_id: EventId = prepared.id.into();
                                activity
                                    .address_announcements
                                    .insert(payload.derivation_index, message_id);
                                sink.send(ChannelEvent::ReceiveAddress {
                                    message_id,
                                    author: prepared.pubkey.into(),
                                    timestamp: prepared.created_at.as_secs(),
                                    pending: true,
                                    derivation_index: payload.derivation_index,
                                    memo: payload.memo.clone(),
                                });
                                match runner_handle_for_task.publish_prepared(prepared).await {
                                    Ok(outcome) if outcome.any_relay_success() => {
                                        sink.send(ChannelEvent::MessageSent { message_id });
                                    }
                                    Ok(outcome) => {
                                        let reason = format!(
                                            "no relay accepted: {:?}",
                                            outcome.relay_failed
                                        );
                                        tracing::error!(%reason, "no relay accepted receive-address message");
                                        sink.send(ChannelEvent::ReceiveAddressSendFailed {
                                            message_id, reason,
                                        });
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "failed to send receive-address message");
                                        sink.send(ChannelEvent::ReceiveAddressSendFailed {
                                            message_id, reason: e.to_string(),
                                        });
                                    }
                                }
                            }
                            Some(ChannelCommand::NotifyTxObserved(tx)) => {
                                handle_notify_tx_observed(&tx, &mut activity, &sink);
                            }
                            None => break,
                        }
                    }
                    event = events.recv() => {
                        match event {
                            Some(ChannelRunnerEvent::ChatMessage { message_id, author, content, timestamp, reply_to }) => {
                                sink.send(ChannelEvent::ChatMessage {
                                    message_id,
                                    author,
                                    content,
                                    timestamp,
                                    reply_to,
                                    pending: false,
                                });
                            }
                            Some(ChannelRunnerEvent::AppEvent { inner_event, ack }) => {
                                if inner_event.kind == KIND_FROSTSNAP_SIGNING {
                                    match process_signing_inner_event(
                                        &inner_event,
                                        &mut tree,
                                    ) {
                                        Ok((signing_evts, timer_acts)) => {
                                            dispatch_signing_output(
                                                signing_evts,
                                                timer_acts,
                                                false,
                                                &sink,
                                                &mut timers,
                                                &mut activity,
                                            );
                                        }
                                        Err(err_event) => sink.send(err_event),
                                    }
                                } else if inner_event.kind == KIND_FROSTSNAP_RECEIVE_ADDRESS {
                                    match crate::channel_runner::decode_bincode::<ReceiveAddressPayload>(&inner_event) {
                                        Ok(payload) => {
                                            activity.address_announcements.insert(
                                                payload.derivation_index,
                                                inner_event.id.into(),
                                            );
                                            sink.send(ChannelEvent::ReceiveAddress {
                                                message_id: inner_event.id.into(),
                                                author: inner_event.pubkey.into(),
                                                timestamp: inner_event.created_at.as_secs(),
                                                pending: false,
                                                derivation_index: payload.derivation_index,
                                                memo: payload.memo,
                                            });
                                        }
                                        Err(e) => {
                                            sink.send(ChannelEvent::Error {
                                                event_id: inner_event.id.into(),
                                                author: inner_event.pubkey.into(),
                                                timestamp: inner_event.created_at.as_secs(),
                                                reason: format!(
                                                    "failed to decode receive-address payload: {e}"
                                                ),
                                            });
                                        }
                                    }
                                }
                                // Signal the dispatch ack AFTER
                                // `process_signing_inner_event` +
                                // `dispatch_signing_output` — so a
                                // local `dispatch` caller only
                                // resolves once the sink has fired
                                // with the tree update. `None` for
                                // events arriving via subscription.
                                if let Some(ack) = ack {
                                    let _ = ack.send(());
                                }
                            }
                            Some(ChannelRunnerEvent::MembersChanged) => {
                                let members = runner_handle_for_task.members();
                                sink.send(ChannelEvent::GroupMetadata {
                                    members: members
                                        .iter()
                                        .map(|(pubkey, profile)| GroupMember {
                                            pubkey: *pubkey,
                                            profile: profile.clone(),
                                        })
                                        .collect(),
                                });
                            }
                            Some(ChannelRunnerEvent::MemberProfileUpdated { pubkey, profile }) => {
                                sink.send(ChannelEvent::MemberProfileUpdated { pubkey, profile });
                            }
                            Some(ChannelRunnerEvent::CreationEventReceived) => {
                                if let Some(creation_event) = runner_handle_for_task.creation_event() {
                                    match crate::channel_runner::decode_bincode::<crate::ChannelInitData>(&creation_event) {
                                        Ok(init_data) => {
                                            sink.send(ChannelEvent::ChannelState {
                                                participants: init_data
                                                    .participants
                                                    .iter()
                                                    .map(ChannelParticipant::from)
                                                    .collect(),
                                            });
                                        }
                                        Err(e) => {
                                            tracing::warn!(error = %e, "failed to decode channel init data");
                                        }
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    Some(request_id) = next_timer(&timers) => {
                        timers.remove(&request_id);
                        if let Some(signing_event) = tree.timer_expired(request_id)
                        {
                            sink.send(ChannelEvent::Signing {
                                event: signing_event,
                                pending: false,
                            });
                        }
                    }
                }
            }
        });

        Ok(ChannelHandle {
            cmd_tx,
            runner_handle,
        })
    }
}

enum ChannelCommand {
    /// Chat message to be published optimistically (with a `pending`
    /// `ChatMessage` already on the sink) and then reported via
    /// `MessageSent` / `MessageSendFailed`.
    SendPreparedMessage(Event),
    /// Receive-address share message — same optimistic→sent/failed
    /// lifecycle as chat, but emits `ChannelEvent::ReceiveAddress`
    /// (pending=true) on entry and `ReceiveAddressSendFailed` on
    /// failure. Success reuses `MessageSent { id }`.
    SendPreparedReceiveAddress(Event, ReceiveAddressPayload),
    /// Dart pumps each wallet-stream tx through this to drive the
    /// runner's tx-observation correlation fold. Local-only — no
    /// relay traffic.
    NotifyTxObserved(frostsnap_coordinator::bitcoin::wallet::Transaction),
}

/// Per-tx state held by the runner. Resolved correlation fields are
/// set ONCE on first observation and never updated (chronology-only
/// — no back-patching when later chat events arrive).
struct ObservedTx {
    /// BDK's `first_seen` — chat-timeline anchor for the Mempool
    /// emission. Stable across re-observation.
    first_seen: Option<u64>,
    confirmation_time: Option<u64>,
    address_reveal_event: Option<EventId>,
    signing_start_event: Option<EventId>,
    mempool_emitted: bool,
    confirmation_emitted: bool,
}

/// Runner-internal correlation state. Folded from nostr events
/// (`address_announcements`, `signing_starts_by_txid` — replay-
/// derived) and Dart-pumped wallet observations (`observed_txs` —
/// driven by local snapshots, not bitwise-identical across
/// participants).
#[derive(Default)]
struct ActivityState {
    address_announcements: BTreeMap<u32, EventId>,
    signing_starts_by_txid: BTreeMap<String, EventId>,
    observed_txs: BTreeMap<String, ObservedTx>,
}

/// Handle for sending messages to an active channel. Chat goes via
/// `cmd_tx` so the task can drive its "optimistic emit → publish →
/// final status" sink flow. Signing protocol events go direct to
/// `runner_handle.dispatch` — the runner gates local apply on relay
/// OK and the task's AppEvent branch handles the echo identically
/// to a peer event.
#[derive(Clone)]
pub struct ChannelHandle {
    cmd_tx: mpsc::Sender<ChannelCommand>,
    pub runner_handle: ChannelRunnerHandle,
}

impl ChannelHandle {
    pub async fn send_message(
        &self,
        content: String,
        reply_to: Option<EventId>,
        keys: &Keys,
    ) -> Result<EventId> {
        let draft = ChannelMessageDraft::text(content, reply_to.into_iter().collect());
        let prepared = draft.prepare(keys).await?;
        let message_id: EventId = prepared.id.into();

        self.cmd_tx
            .send(ChannelCommand::SendPreparedMessage(prepared))
            .await
            .map_err(|_| anyhow!("channel closed"))?;

        Ok(message_id)
    }

    pub async fn send_receive_address(
        &self,
        keys: &Keys,
        derivation_index: u32,
        memo: String,
    ) -> Result<EventId> {
        let payload = ReceiveAddressPayload {
            derivation_index,
            memo,
        };
        let draft = ChannelMessageDraft::app(KIND_FROSTSNAP_RECEIVE_ADDRESS, &payload, vec![])?;
        let prepared = draft.prepare(keys).await?;
        let message_id: EventId = prepared.id.into();

        self.cmd_tx
            .send(ChannelCommand::SendPreparedReceiveAddress(
                prepared, payload,
            ))
            .await
            .map_err(|_| anyhow!("channel closed"))?;

        Ok(message_id)
    }

    pub async fn send_sign_request(
        &self,
        keys: &Keys,
        sign_task: WireSignTask,
        message: String,
    ) -> Result<EventId> {
        let signing_msg = SigningMessage::Request { sign_task, message };
        self.send_signing_event(keys, &signing_msg, None).await
    }

    /// Pump a wallet-stream tx into the runner. Drives the
    /// tx-observation correlation fold. Local-only — no relay
    /// traffic. Idempotent: repeated notifies of the same tx
    /// don't re-emit events.
    pub async fn notify_tx_observed(
        &self,
        tx: frostsnap_coordinator::bitcoin::wallet::Transaction,
    ) -> Result<()> {
        self.cmd_tx
            .send(ChannelCommand::NotifyTxObserved(tx))
            .await
            .map_err(|_| anyhow!("channel closed"))?;
        Ok(())
    }

    /// Publish a sign offer. Every offer's reply_to is the request's event
    /// id — the flat-CRDT protocol has no chain topology for offers.
    pub async fn send_sign_offer(
        &self,
        keys: &Keys,
        request_id: EventId,
        binonces: Vec<ParticipantBinonces>,
    ) -> Result<EventId> {
        let message = SigningMessage::Offer { binonces };
        self.send_signing_event(keys, &message, Some(request_id))
            .await
    }

    /// Publish a signing partial. `offer_subset` names the offer events
    /// whose binonces the caller signed against — combiners resolve each
    /// event_id to its offer and recover `session_id` from those binonces.
    pub async fn send_sign_partial(
        &self,
        keys: &Keys,
        request_id: EventId,
        offer_subset: Vec<EventId>,
        signature_shares: ParticipantSignatureShares,
    ) -> Result<EventId> {
        use self::events::WireEventId;
        let message = SigningMessage::Partial {
            offer_subset: offer_subset.into_iter().map(WireEventId::from).collect(),
            signature_shares,
        };
        self.send_signing_event(keys, &message, Some(request_id))
            .await
    }

    pub async fn send_sign_cancel(&self, keys: &Keys, request_id: EventId) -> Result<EventId> {
        let message = SigningMessage::Cancel;
        self.send_signing_event(keys, &message, Some(request_id))
            .await
    }

    async fn send_signing_event(
        &self,
        keys: &Keys,
        message: &SigningMessage,
        reply_to: Option<EventId>,
    ) -> Result<EventId> {
        let draft = ChannelMessageDraft::app(
            KIND_FROSTSNAP_SIGNING,
            message,
            reply_to.into_iter().collect(),
        )?;
        // Dispatch: publishes to relays, gates on ≥1 relay OK, and
        // — on success — feeds the event through the runner's AppEvent
        // path so the signing tree + sink update identically to a
        // peer-received event. `.await` resolves only after the Sink
        // has fired. No optimistic local apply; no retry queue.
        let outcome = self.runner_handle.dispatch(keys, draft).await?;
        if !outcome.any_relay_success() {
            return Err(anyhow!(
                "no relay accepted the signing event: {:?}",
                outcome.relay_failed
            ));
        }
        Ok(outcome.inner_event_id)
    }
}

// ============================================================================
// Timer helpers
// ============================================================================

fn apply_timer_actions(timers: &mut HashMap<EventId, Instant>, actions: Vec<TimerAction>) {
    for action in actions {
        match action {
            TimerAction::Set {
                request_id,
                duration,
            } => {
                timers.insert(request_id, Instant::now() + duration);
            }
            TimerAction::Cancel { request_id } => {
                timers.remove(&request_id);
            }
        }
    }
}

fn next_timer(
    timers: &HashMap<EventId, Instant>,
) -> Pin<Box<dyn Future<Output = Option<EventId>> + Send + '_>> {
    if let Some((&request_id, &deadline)) = timers.iter().min_by_key(|(_, inst)| **inst) {
        Box::pin(async move {
            sleep_until(deadline).await;
            Some(request_id)
        })
    } else {
        Box::pin(future::pending())
    }
}

// ============================================================================
// Signing event processing
// ============================================================================

/// Decode a signing inner event, feed it to the tree, and return the
/// resulting signing events + timer actions. Decode errors are returned as
/// `ChannelEvent::Error` in the first element of the tuple (signing events
/// are empty in that case).
fn process_signing_inner_event(
    inner_event: &Event,
    tree: &mut SigningEventTree,
) -> Result<(Vec<SigningEvent>, Vec<TimerAction>), ChannelEvent> {
    let meta = EventMeta::from_event(inner_event);

    let err = |reason: String| ChannelEvent::Error {
        event_id: meta.event_id,
        author: meta.author,
        timestamp: meta.timestamp,
        reason,
    };

    let message: SigningMessage = decode_bincode(inner_event).map_err(|e| {
        tracing::warn!(event_id = %meta.event_id, error = %e, "failed to decode signing message");
        err(format!("failed to decode signing message: {e}"))
    })?;

    let wire = match message {
        SigningMessage::Request { sign_task, message } => WireEvent::Request { sign_task, message },
        SigningMessage::Offer { binonces } => {
            let request_id = extract_e_tag(inner_event)
                .ok_or_else(|| err("signing offer missing e-tag".into()))?;
            WireEvent::Offer {
                request_id,
                binonces,
            }
        }
        SigningMessage::Partial {
            offer_subset,
            signature_shares,
        } => {
            let request_id = extract_e_tag(inner_event)
                .ok_or_else(|| err("signing partial missing e-tag".into()))?;
            WireEvent::Partial {
                request_id,
                offer_subset: offer_subset.into_iter().map(EventId::from).collect(),
                signature_shares,
            }
        }
        SigningMessage::Cancel => {
            let request_id = extract_e_tag(inner_event)
                .ok_or_else(|| err("signing cancel missing e-tag".into()))?;
            WireEvent::Cancel { request_id }
        }
    };

    Ok(tree.ingest_wire(meta, wire))
}

/// Wallet observation → correlation fold + emit. Resolves the
/// receive-address and signing-start correlations once at first
/// observation (chronology-only — never back-patches). Emits
/// Mempool / Confirmed events once each per tx.
fn handle_notify_tx_observed(
    tx: &frostsnap_coordinator::bitcoin::wallet::Transaction,
    activity: &mut ActivityState,
    sink: &impl Sink<ChannelEvent>,
) {
    use events::ObservationKind;

    let txid = tx.txid.to_string();
    let first_seen = tx.first_seen;
    let confirmation_time = tx.confirmation_time.as_ref().map(|c| c.time);

    let entry = activity
        .observed_txs
        .entry(txid.clone())
        .or_insert_with(|| {
            // Mine-EXTERNAL-output derivation indices: iterate
            // `tx.inner.output` (NOT `tx.is_mine.values()` — that map
            // mixes input + output owned scripts), and filter to the
            // external keychain (receive addresses). Internal change
            // outputs share the index space but are never the target
            // of an `address_announcements` entry, so excluding them
            // here prevents false-positive quote attachment on
            // outgoing-with-change txs. Lowest index wins for receive
            // correlation.
            let mut mine_output_indices: Vec<u32> = tx
                .inner
                .output
                .iter()
                .filter_map(|txout| {
                    tx.is_mine
                        .get(&txout.script_pubkey)
                        .and_then(|(keychain, idx)| {
                            (keychain.keychain == frostsnap_core::tweak::Keychain::External)
                                .then_some(*idx)
                        })
                })
                .collect();
            mine_output_indices.sort();
            let address_reveal_event = mine_output_indices
                .iter()
                .find_map(|idx| activity.address_announcements.get(idx).copied());
            let signing_start_event = activity.signing_starts_by_txid.get(&txid).copied();
            ObservedTx {
                first_seen,
                confirmation_time,
                address_reveal_event,
                signing_start_event,
                mempool_emitted: false,
                confirmation_emitted: false,
            }
        });

    // Refresh chain state fields — they can transition None→Some
    // across notifies. `first_seen` may also shift EARLIER if BDK
    // discovers an earlier sighting (`update_first_seen` only writes
    // strictly-earlier values).
    entry.first_seen = match (entry.first_seen, first_seen) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    };
    if entry.confirmation_time.is_none() {
        entry.confirmation_time = confirmation_time;
    }

    if let Some(fs) = entry.first_seen {
        if !entry.mempool_emitted {
            entry.mempool_emitted = true;
            sink.send(ChannelEvent::TxObservation {
                txid: txid.clone(),
                kind: ObservationKind::Mempool,
                timestamp: fs,
                address_reveal_event: entry.address_reveal_event,
                signing_start_event: entry.signing_start_event,
            });
        }
    }
    if let Some(ct) = entry.confirmation_time {
        if !entry.confirmation_emitted {
            entry.confirmation_emitted = true;
            // Confirmed timestamp must not sort before the Mempool
            // entry. Miner's block.time has ±2h fudge factor and can
            // be earlier than our first_seen. Clamp to
            // first_seen + 1 second so confirmed always lands
            // strictly after the mempool card.
            let timestamp = match entry.first_seen {
                Some(fs) => ct.max(fs.saturating_add(1)),
                None => ct,
            };
            sink.send(ChannelEvent::TxObservation {
                txid,
                kind: ObservationKind::Confirmed,
                timestamp,
                address_reveal_event: entry.address_reveal_event,
                signing_start_event: entry.signing_start_event,
            });
        }
    }
}

/// Send signing events + timer actions to sink/timers. The `pending` flag
/// marks the first signing event as a local optimistic echo. Folds
/// `SigningEvent::Request` entries into `activity.signing_starts_by_txid`
/// for chat→tx correlation on later wallet observations.
fn dispatch_signing_output(
    signing_events: Vec<SigningEvent>,
    timer_actions: Vec<TimerAction>,
    pending: bool,
    sink: &impl Sink<ChannelEvent>,
    timers: &mut HashMap<EventId, Instant>,
    activity: &mut ActivityState,
) {
    for (i, event) in signing_events.into_iter().enumerate() {
        if let SigningEvent::Request {
            event_id,
            sign_task: frostsnap_core::WireSignTask::BitcoinTransaction(template),
            ..
        } = &event
        {
            let txid = template.txid().to_string();
            activity
                .signing_starts_by_txid
                .entry(txid)
                .or_insert(*event_id);
        }
        sink.send(ChannelEvent::Signing {
            event,
            pending: pending && i == 0,
        });
    }
    apply_timer_actions(timers, timer_actions);
}
