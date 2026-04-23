mod events;
mod tree;

use events::SigningMessage;
pub use events::{ChannelEvent, ConfirmedSubsetEntry, ConnectionState, GroupMember, SigningEvent};

use self::tree::{SigningEventTree, TimerAction, WireEvent};
use crate::{
    channel::ChannelKeys,
    channel_runner::{
        decode_bincode, extract_e_tag, ChannelMessageDraft, ChannelRunner, ChannelRunnerEvent,
        EventMeta,
    },
};
use anyhow::{anyhow, Result};
use frostsnap_coordinator::Sink;
use frostsnap_core::{
    coordinator::{KeyContext, ParticipantBinonces, ParticipantSignatureShares},
    WireSignTask,
};
use nostr_sdk::{Client, Event, EventId, Keys, Kind};
use std::collections::HashMap;
use std::future::{self, Future};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep_until, Instant};

const KIND_FROSTSNAP_SIGNING: Kind = Kind::Custom(9001);

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
    ) -> Result<ChannelHandle> {
        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connecting));

        // Build channel init inner event if we have init data to publish
        let mut runner = ChannelRunner::new(self.channel_keys.clone());
        if let Some(init_data) = &self.init_data {
            let init_inner = init_data.to_channel_creation_event()?;
            runner = runner.with_init_event(init_inner);
        }

        let (runner_handle, mut events) = runner.run(client).await?;

        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ChannelCommand>(32);

        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connected));

        let runner_handle_for_task = runner_handle.clone();
        let key_context = self.key_context.clone();
        let settling_window = self.settling_window;
        tokio::spawn(async move {
            let mut tree = SigningEventTree::new(key_context, settling_window);
            let mut timers: HashMap<EventId, Instant> = HashMap::new();
            // Signing events that couldn't reach any relay on the first try.
            // Retried in-process with exponential backoff until success.
            let mut pending_retries: Vec<PendingSigningSend> = Vec::new();

            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(ChannelCommand::SendPreparedMessage(prepared)) => {
                                let message_id = prepared.id;

                                sink.send(ChannelEvent::from_inner_chat_message(
                                    &prepared,
                                    true,
                                ));

                                match runner_handle_for_task.send_prepared(prepared).await {
                                    Ok(()) => {
                                        sink.send(ChannelEvent::MessageSent { message_id });
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "failed to send message");
                                        sink.send(ChannelEvent::MessageSendFailed {
                                            message_id,
                                            reason: e.to_string(),
                                        });
                                    }
                                }
                            }
                            Some(ChannelCommand::SendSigningEvent(prepared)) => {
                                let message_id = prepared.id;
                                match process_signing_inner_event(
                                    &prepared,
                                    &mut tree,
                                ) {
                                    Ok((signing_evts, timer_acts)) => {
                                        dispatch_signing_output(
                                            signing_evts,
                                            timer_acts,
                                            true,
                                            &sink,
                                            &mut timers,
                                        );
                                    }
                                    Err(err_event) => sink.send(err_event),
                                }
                                match runner_handle_for_task.send_prepared(prepared.clone()).await {
                                    Ok(()) => {
                                        sink.send(ChannelEvent::MessageSent { message_id });
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            error = %e,
                                            "signing event send failed; will retry",
                                        );
                                        pending_retries.push(PendingSigningSend {
                                            event: prepared,
                                            next_retry_at: Instant::now()
                                                + retry_backoff(1),
                                            attempts: 1,
                                        });
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    Some(index) = next_retry_due(&pending_retries) => {
                        let item = pending_retries.swap_remove(index);
                        let message_id = item.event.id;
                        match runner_handle_for_task.send_prepared(item.event.clone()).await {
                            Ok(()) => {
                                tracing::info!(
                                    attempts = item.attempts,
                                    "signing event retry succeeded",
                                );
                                sink.send(ChannelEvent::MessageSent { message_id });
                            }
                            Err(e) => {
                                let attempts = item.attempts + 1;
                                tracing::warn!(
                                    error = %e,
                                    attempts,
                                    "signing event retry failed; will try again",
                                );
                                pending_retries.push(PendingSigningSend {
                                    event: item.event,
                                    next_retry_at: Instant::now()
                                        + retry_backoff(attempts),
                                    attempts,
                                });
                            }
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
                            Some(ChannelRunnerEvent::AppEvent { inner_event }) => {
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
                                            );
                                        }
                                        Err(err_event) => sink.send(err_event),
                                    }
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
                            Some(ChannelRunnerEvent::CreationEventReceived) => {}
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

        Ok(ChannelHandle { cmd_tx })
    }
}

enum ChannelCommand {
    SendPreparedMessage(Event),
    SendSigningEvent(Event),
}

/// Handle for sending messages to an active channel.
#[derive(Clone)]
pub struct ChannelHandle {
    cmd_tx: mpsc::Sender<ChannelCommand>,
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
        let message_id = prepared.id;

        self.cmd_tx
            .send(ChannelCommand::SendPreparedMessage(prepared))
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
        let prepared = draft.prepare(keys).await?;
        let event_id = prepared.id;
        self.cmd_tx
            .send(ChannelCommand::SendSigningEvent(prepared))
            .await
            .map_err(|_| anyhow!("channel closed"))?;
        Ok(event_id)
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

/// One entry in the in-process retry queue for signing events whose initial
/// relay publish failed.
struct PendingSigningSend {
    event: Event,
    next_retry_at: Instant,
    attempts: u32,
}

/// Sleeps until the earliest pending retry is due, then returns its index
/// into `pending`. Returns `Pending` forever when the queue is empty so the
/// `tokio::select!` arm stays quiet until something is enqueued.
fn next_retry_due(
    pending: &[PendingSigningSend],
) -> Pin<Box<dyn Future<Output = Option<usize>> + Send + '_>> {
    if let Some((index, _)) = pending
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| p.next_retry_at)
    {
        let deadline = pending[index].next_retry_at;
        Box::pin(async move {
            sleep_until(deadline).await;
            Some(index)
        })
    } else {
        Box::pin(future::pending())
    }
}

/// Exponential backoff capped at 30s: 2s, 4s, 8s, 16s, 30s, 30s, …
fn retry_backoff(attempts: u32) -> Duration {
    let secs = (1u64 << attempts.min(5)).min(30);
    Duration::from_secs(secs)
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

/// Send signing events + timer actions to sink/timers. The `pending` flag
/// marks the first signing event as a local optimistic echo.
fn dispatch_signing_output(
    signing_events: Vec<SigningEvent>,
    timer_actions: Vec<TimerAction>,
    pending: bool,
    sink: &impl Sink<ChannelEvent>,
    timers: &mut HashMap<EventId, Instant>,
) {
    for (i, event) in signing_events.into_iter().enumerate() {
        sink.send(ChannelEvent::Signing {
            event,
            pending: pending && i == 0,
        });
    }
    apply_timer_actions(timers, timer_actions);
}
