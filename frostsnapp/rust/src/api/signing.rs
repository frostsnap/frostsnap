use super::super_wallet::SuperWallet;
use super::{
    bitcoin::{BitcoinNetwork, RTransaction, Transaction},
    coordinator::Coordinator,
};
use crate::api::broadcast::{BehaviorBroadcast, BehaviorBroadcastSubscription, StartError};
use crate::frb_generated::StreamSink;
use crate::sink_wrap::SinkWrap;
use anyhow::{anyhow, Result};
use bitcoin::hex::DisplayHex;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::persist::Persisted;
pub use frostsnap_coordinator::signing::SigningState;
use frostsnap_coordinator::signing::{DispatcherCommand, SigningDispatcher};
pub use frostsnap_core::bitcoin_transaction::TransactionTemplate;
pub use frostsnap_core::coordinator::signing::RemoteSignSessionId;
pub use frostsnap_core::coordinator::ActiveSignSession;
use frostsnap_core::coordinator::FrostCoordinator;
pub use frostsnap_core::coordinator::{
    ParticipantBinonces, ParticipantSignatureShares, SignSessionProgress, StartSign,
};
use frostsnap_core::MasterAppkey;
pub use frostsnap_core::WireSignTask;
use frostsnap_core::{
    message::EncodedSignature, AccessStructureRef, DeviceId, KeyId, SignSessionId, SymmetricKey,
};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::sync::{mpsc::Sender, Arc, Mutex};
use tracing::{event, Level};

#[frb(mirror(RemoteSignSessionId), non_opaque)]
pub struct _RemoteSignSessionId(pub [u8; 32]);

/// An outgoing Bitcoin transaction that has not been successfully broadcast.
///
/// May be signed or unsigned, but is guaranteed to have a signing session associated with it.
#[derive(Debug, Clone)]
#[frb]
pub struct UnbroadcastedTx {
    pub tx: Transaction,
    pub session_id: SignSessionId,
    /// Some for active (incomplete) sign sessions.
    pub active_session: Option<ActiveSignSession>,
}

impl UnbroadcastedTx {
    #[frb(sync)]
    pub fn is_signed(&self) -> bool {
        self.active_session.is_none()
    }
}

#[derive(Debug, Clone)]
#[frb(non_opaque)]
pub enum SigningDetails {
    Message {
        message: String,
    },
    Transaction {
        transaction: crate::api::bitcoin::Transaction,
    },
    Nostr {
        id: String,
        content: String,
        hash_bytes: String,
    },
}

#[frb(mirror(SigningState), unignore)]
pub struct _SigningState {
    pub session_id: SignSessionId,
    pub got_shares: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    pub finished_signatures: Option<Vec<EncodedSignature>>,
    pub aborted: Option<String>,
    pub connected_but_need_request: Vec<DeviceId>,
}

#[frb(mirror(ActiveSignSession), unignore)]
pub struct _ActiveSignSession {
    pub progress: Vec<SignSessionProgress>,
    pub init: StartSign,
    pub key_id: KeyId,
    pub sent_req_to_device: HashSet<DeviceId>,
}

pub trait ActiveSignSessionExt {
    #[frb(sync)]
    fn state(&self) -> SigningState;
    #[frb(sync)]
    fn details(&self) -> SigningDetails;
    #[frb(sync)]
    fn access_structure_ref(&self) -> AccessStructureRef;
}

impl ActiveSignSessionExt for ActiveSignSession {
    #[frb(sync)]
    fn state(&self) -> SigningState {
        let session_id = self.session_id();
        let session_init = &self.init;
        let got_shares = self.received_from();
        let state = SigningState {
            session_id,
            got_shares: got_shares.into_iter().collect(),
            needed_from: session_init.local_nonces.keys().copied().collect(),
            finished_signatures: None,
            aborted: None,
            connected_but_need_request: Default::default(),
        };

        state
    }

    #[frb(sync)]
    fn details(&self) -> SigningDetails {
        self.init.group_request.sign_task.signing_details()
    }

    #[frb(sync)]
    fn access_structure_ref(&self) -> AccessStructureRef {
        ActiveSignSession::access_structure_ref(self)
    }
}

pub trait WireSignTaskExt {
    #[frb(sync)]
    fn signing_details(&self) -> SigningDetails;
}

/// Build a `WireSignTask` for signing a bitcoin transaction. Needed on the
/// Dart side because `WireSignTask` is opaque — Dart can't construct the
/// inner variants directly.
#[frb(sync)]
pub fn wire_sign_task_bitcoin_transaction(unsigned_tx: &UnsignedTx) -> WireSignTask {
    WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.clone())
}

/// Build a `WireSignTask` for signing a plain test message.
#[frb(sync)]
pub fn wire_sign_task_test(message: String) -> WireSignTask {
    WireSignTask::Test { message }
}

impl WireSignTaskExt for WireSignTask {
    #[frb(sync)]
    fn signing_details(&self) -> SigningDetails {
        match self {
            WireSignTask::Test { message } => SigningDetails::Message {
                message: message.clone(),
            },
            WireSignTask::Nostr { event } => SigningDetails::Nostr {
                id: event.id.clone(),
                content: event.content.clone(),
                hash_bytes: event.hash_bytes.to_lower_hex_string(),
            },
            WireSignTask::BitcoinTransaction(tx_temp) => {
                let raw_tx = tx_temp.to_rust_bitcoin_tx();
                let txid = raw_tx.compute_txid();
                let is_mine = tx_temp
                    .iter_locally_owned_inputs()
                    .map(|(_, _, spk)| (spk.spk(), spk.bip32_path.index))
                    .chain(
                        tx_temp
                            .iter_locally_owned_outputs()
                            .map(|(_, _, spk)| (spk.spk(), spk.bip32_path.index)),
                    )
                    .collect::<HashMap<_, _>>();
                let prevouts = tx_temp
                    .inputs()
                    .iter()
                    .map(|input| (input.outpoint(), input.txout()))
                    .collect::<HashMap<bitcoin::OutPoint, bitcoin::TxOut>>();
                SigningDetails::Transaction {
                    transaction: Transaction {
                        inner: raw_tx,
                        txid: txid.to_string(),
                        confirmation_time: None,
                        last_seen: None,
                        prevouts,
                        is_mine,
                    },
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct UnsignedTx {
    pub template_tx: TransactionTemplate,
}

impl UnsignedTx {
    #[frb(sync)]
    pub fn txid(&self) -> String {
        self.template_tx.txid().to_string()
    }

    #[frb(sync, type_64bit_int)]
    pub fn fee(&self) -> Option<u64> {
        self.template_tx.fee()
    }

    #[frb(sync)]
    pub fn feerate(&self) -> Option<f64> {
        self.template_tx.feerate()
    }

    #[frb(sync)]
    pub fn details(&self, super_wallet: &SuperWallet, master_appkey: MasterAppkey) -> Transaction {
        let super_wallet = super_wallet.inner.lock().unwrap();
        let raw_tx = self.template_tx.to_rust_bitcoin_tx();
        let txid = raw_tx.compute_txid();
        Transaction {
            txid: txid.to_string(),
            confirmation_time: None,
            last_seen: None,
            prevouts: super_wallet
                .get_prevouts(raw_tx.input.iter().map(|txin| txin.previous_output)),
            is_mine: raw_tx
                .output
                .iter()
                .chain(
                    super_wallet
                        .get_prevouts(raw_tx.input.iter().map(|txin| txin.previous_output))
                        .values(),
                )
                .filter_map(|txout| {
                    let spk = txout.script_pubkey.clone();
                    super_wallet
                        .spk_index(master_appkey, spk.clone())
                        .map(|index| (spk, index))
                })
                .collect::<HashMap<_, _>>(),
            inner: raw_tx,
        }
    }

    #[frb(sync)]
    pub fn complete(&self, signatures: Vec<EncodedSignature>) -> SignedTx {
        let mut tx = self.template_tx.to_rust_bitcoin_tx();
        for (txin, signature) in tx.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                    .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.witness = witness;
        }

        SignedTx {
            signed_tx: tx,
            unsigned_tx: self.clone(),
        }
    }

    #[frb(sync)]
    pub fn effect(
        &self,
        master_appkey: MasterAppkey,
        network: BitcoinNetwork,
    ) -> Result<EffectOfTx> {
        use frostsnap_core::bitcoin_transaction::RootOwner;
        let fee = self
            .template_tx
            .fee()
            .ok_or(anyhow!("invalid transaction"))?;
        let mut net_value = self.template_tx.net_value();
        let value_for_this_key = net_value
            .remove(&RootOwner::Local(master_appkey))
            .ok_or(anyhow!("this transaction has no effect on this key"))?;

        let foreign_receiving_addresses = net_value
            .into_iter()
            .filter_map(|(owner, value)| match owner {
                RootOwner::Local(_) => Some(Err(anyhow!(
                    "we don't support spending from multiple different keys"
                ))),
                RootOwner::Foreign(spk) => {
                    if value > 0 {
                        Some(Ok((
                            bitcoin::Address::from_script(spk.as_script(), network)
                                .expect("will have address form")
                                .to_string(),
                            value as u64,
                        )))
                    } else {
                        None
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(EffectOfTx {
            net_value: value_for_this_key,
            fee,
            feerate: self.template_tx.feerate(),
            foreign_receiving_addresses,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SignedTx {
    pub signed_tx: RTransaction,
    pub unsigned_tx: UnsignedTx,
}

impl SignedTx {
    #[frb(sync)]
    pub fn txid(&self) -> String {
        self.signed_tx.compute_txid().to_string()
    }

    #[frb(sync)]
    pub fn effect(
        &self,
        master_appkey: MasterAppkey,
        network: BitcoinNetwork,
    ) -> Result<EffectOfTx> {
        self.unsigned_tx.effect(master_appkey, network)
    }
}

/// Opaque handle to an in-flight signing session. Returned by the entry
/// points that install a `SigningDispatcher` on the coordinator's ui_stack.
///
/// All operations that affect a specific signing session (subscribing to
/// state, pushing a device sign request, cancelling) go through the handle.
/// That makes "which dispatcher should receive this?" a compile-time
/// guarantee instead of a `ui_stack.get_mut::<T>()` runtime lookup.
#[frb(opaque)]
pub struct SigningSessionHandle {
    coordinator: Arc<Mutex<Persisted<FrostCoordinator>>>,
    db: Arc<Mutex<Connection>>,
    dispatcher_tx: Sender<DispatcherCommand>,
    signing_session_signals: crate::coordinator::SignalMap,
    key_id: KeyId,
    session_id: SignSessionId,
    /// The sink the dispatcher writes `SigningState` updates into; also
    /// the subscribable stream source exposed via `sub_state`. Using a
    /// `BehaviorBroadcast` means (a) multiple Dart subscribers fan out,
    /// and (b) a fresh subscriber receives the current snapshot
    /// immediately on `start()` instead of waiting for the next emission.
    state_broadcast: BehaviorBroadcast<SigningState>,
    /// `None` for local signing. `Some` for remote — carries the
    /// round-scoped context (the id of the remote reservation + the full
    /// participant binonce set). Per-device `access_structure_ref` and
    /// `sign_task` were committed at `offer_to_sign` time and live on the
    /// `RemoteSignSession` in core; we don't duplicate them on the handle.
    /// Held here so the per-device action
    /// method can take just `(device_id, encryption_key)` and the caller
    /// can't supply values that disagree with `session_id` / `key_id`.
    remote: Option<RemoteSigningContext>,
}

struct RemoteSigningContext {
    remote_sign_session_id: RemoteSignSessionId,
    all_binonces: Vec<ParticipantBinonces>,
}

/// FFI-exposed concrete wrapper over `BehaviorBroadcastSubscription<SigningState>`.
/// Generics don't cross the FRB boundary, so we wrap the concrete type.
pub struct SigningStateBroadcastSubscription(
    pub(crate) BehaviorBroadcastSubscription<SigningState>,
);

impl SigningStateBroadcastSubscription {
    #[frb(sync)]
    pub fn id(&self) -> u32 {
        self.0._id()
    }

    #[frb(sync)]
    pub fn is_running(&self) -> bool {
        self.0._is_running()
    }

    #[frb(sync)]
    pub fn start(&self, sink: StreamSink<SigningState>) -> std::result::Result<(), StartError> {
        self.0._start(sink)
    }

    #[frb(sync)]
    pub fn stop(&self) -> bool {
        self.0._stop()
    }
}

impl SigningSessionHandle {
    #[frb(sync)]
    pub fn session_id(&self) -> SignSessionId {
        self.session_id
    }

    #[frb(sync)]
    pub fn key_id(&self) -> KeyId {
        self.key_id
    }

    /// Subscribe to `SigningState` updates. Returns a subscription handle;
    /// call `.start(sink)` on it to begin receiving updates. Multiple
    /// concurrent subscriptions are supported (fan-out). Each fresh
    /// `.start()` immediately emits the cached current state before
    /// streaming further updates.
    #[frb(sync)]
    pub fn sub_state(&self) -> SigningStateBroadcastSubscription {
        SigningStateBroadcastSubscription(self.state_broadcast.subscribe())
    }

    /// Build a `RequestDeviceSign` via core and hand it to this session's
    /// dispatcher. Works for both local and remote sessions — the handle
    /// carries whatever context the core mutation needs.
    pub fn request_device_sign(
        &self,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        let sign_req = {
            let mut db = self.db.lock().unwrap();
            let mut coord = self.coordinator.lock().unwrap();
            coord.staged_mutate(&mut *db, |coordinator| match &self.remote {
                None => Ok(coordinator.request_device_sign(
                    self.session_id,
                    device_id,
                    encryption_key,
                )?),
                Some(ctx) => Ok(coordinator.sign_with_nonce_reservation(
                    ctx.remote_sign_session_id,
                    device_id,
                    &ctx.all_binonces,
                    encryption_key,
                )?),
            })?
        };
        self.dispatcher_tx
            .send(DispatcherCommand::SendSignRequest(Box::new(sign_req)))
            .map_err(|_| anyhow!("signing session has been shut down"))?;

        // For remote signing, poke the per-key "is signing happening?"
        // signal so any Dart subscribers watching that flag update.
        if self.remote.is_some() {
            if let Some(stream) = self
                .signing_session_signals
                .lock()
                .unwrap()
                .get(&self.key_id)
            {
                stream.send(());
            }
        }

        Ok(())
    }

    /// Cancel this specific signing session. Tears down the UI dispatcher
    /// only — does not affect the core-side session state (use
    /// `Coordinator::cancel_sign_session` for that, they are orthogonal
    /// concerns).
    pub fn cancel(&self) {
        let _ = self.dispatcher_tx.send(DispatcherCommand::Cancel);
    }
}

impl Coordinator {
    /// Start signing a test message (not a bitcoin tx).
    pub fn start_signing(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: Vec<DeviceId>,
        message: String,
    ) -> Result<SigningSessionHandle> {
        self.start_signing_inner(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::Test { message },
        )
    }

    /// Start signing a bitcoin transaction.
    pub fn start_signing_tx(
        &self,
        access_structure_ref: AccessStructureRef,
        unsigned_tx: UnsignedTx,
        devices: Vec<DeviceId>,
    ) -> Result<SigningSessionHandle> {
        self.start_signing_inner(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.clone()),
        )
    }

    fn start_signing_inner(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: std::collections::BTreeSet<DeviceId>,
        task: WireSignTask,
    ) -> Result<SigningSessionHandle> {
        let session_id = {
            let mut db = self.0.db.lock().unwrap();
            let mut coordinator = self.0.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                Ok(coordinator.start_sign(
                    access_structure_ref,
                    task,
                    &devices,
                    &mut rand::thread_rng(),
                )?)
            })?
        };

        let state_broadcast = BehaviorBroadcast::<SigningState>::default();
        let (dispatcher, dispatcher_tx) = SigningDispatcher::new(
            devices,
            access_structure_ref.key_id,
            session_id,
            Box::new(state_broadcast.clone()),
        );
        self.0.start_protocol(dispatcher);

        Ok(self.build_handle(
            access_structure_ref.key_id,
            session_id,
            dispatcher_tx,
            state_broadcast,
            None,
        ))
    }

    fn build_handle(
        &self,
        key_id: KeyId,
        session_id: SignSessionId,
        dispatcher_tx: Sender<DispatcherCommand>,
        state_broadcast: BehaviorBroadcast<SigningState>,
        remote: Option<RemoteSigningContext>,
    ) -> SigningSessionHandle {
        SigningSessionHandle {
            coordinator: self.0.coordinator.clone(),
            db: self.0.db.clone(),
            dispatcher_tx,
            signing_session_signals: self.0.signing_session_signals.clone(),
            key_id,
            session_id,
            state_broadcast,
            remote,
        }
    }

    /// Reattach to an in-flight local signing session (e.g. after app
    /// restart). Returns `Err` if the session is no longer in the
    /// coordinator's active store.
    pub fn try_restore_signing_session(
        &self,
        session_id: SignSessionId,
    ) -> Result<SigningSessionHandle> {
        let coordinator = self.0.coordinator.lock().unwrap();
        let active_sign_session = coordinator
            .get_active_sign_session(session_id)
            .ok_or_else(|| anyhow!("this signing session no longer exists"))?;
        let key_id = active_sign_session.key_id;

        let state_broadcast = BehaviorBroadcast::<SigningState>::default();
        let (dispatcher, dispatcher_tx) = SigningDispatcher::restore_signing_session(
            active_sign_session,
            Box::new(state_broadcast.clone()),
        );
        drop(coordinator);
        self.0.start_protocol(dispatcher);

        Ok(self.build_handle(key_id, session_id, dispatcher_tx, state_broadcast, None))
    }

    /// Install a `SigningDispatcher` for a remote signing session and
    /// return a handle bound to it.
    ///
    /// `remote_sign_session_id` identifies the prior `offer_to_sign` that
    /// committed the `access_structure_ref` and `sign_task`; core reads
    /// those from the stored reservation rather than taking them as args.
    /// `all_binonces` is the session-wide participant binonce set (known
    /// after RoundConfirmed); `session_id` is derived from it internally so
    /// it can't disagree with anything else on the handle. `targets` and
    /// `got_signatures` come from the caller's nostr-derived view of who
    /// must sign and who has already signed; the dispatcher folds in
    /// `GotShare` events as our local devices return their shares and
    /// completes once `got_signatures ⊇ targets`.
    pub fn sub_remote_sign_session(
        &self,
        remote_sign_session_id: RemoteSignSessionId,
        all_binonces: Vec<ParticipantBinonces>,
        targets: Vec<DeviceId>,
        got_signatures: Vec<DeviceId>,
    ) -> Result<SigningSessionHandle> {
        anyhow::ensure!(!targets.is_empty(), "targets cannot be empty");

        // Pull the committed context from core (any existing reservation
        // under this id suffices — they all share the same ar + sign_task).
        let (access_structure_ref, sign_task) = {
            let coord = self.0.coordinator.lock().unwrap();
            let mut iter = coord.remote_sign_sessions_by_id(remote_sign_session_id);
            let (_, session) = iter
                .next()
                .ok_or_else(|| anyhow!("no reservation exists for this id"))?;
            (session.access_structure_ref, session.sign_task.clone())
        };

        let session_id = frostsnap_core::message::GroupSignReq::from_binonces(
            sign_task,
            access_structure_ref.access_structure_id,
            &all_binonces,
        )
        .session_id();

        let state_broadcast = BehaviorBroadcast::<SigningState>::default();
        let (dispatcher, dispatcher_tx) = SigningDispatcher::new_remote(
            targets.into_iter().collect(),
            access_structure_ref.key_id,
            session_id,
            got_signatures.into_iter().collect(),
            Box::new(state_broadcast.clone()),
        );
        self.0.start_protocol(dispatcher);

        Ok(self.build_handle(
            access_structure_ref.key_id,
            session_id,
            dispatcher_tx,
            state_broadcast,
            Some(RemoteSigningContext {
                remote_sign_session_id,
                all_binonces,
            }),
        ))
    }

    // ====================================================================
    // Non-handle queries and mutations
    // ====================================================================

    #[frb(sync)]
    pub fn nonces_available(&self, id: DeviceId) -> u32 {
        self.0
            .coordinator
            .lock()
            .unwrap()
            .nonces_available(id)
            .values()
            .copied()
            .max()
            .unwrap_or(0)
    }

    #[frb(sync)]
    pub fn active_signing_session(&self, session_id: SignSessionId) -> Option<ActiveSignSession> {
        self.0
            .coordinator
            .lock()
            .unwrap()
            .active_signing_sessions_by_ssid()
            .get(&session_id)
            .cloned()
    }

    #[frb(sync)]
    pub fn active_signing_sessions(&self, key_id: KeyId) -> Vec<ActiveSignSession> {
        self.0
            .coordinator
            .lock()
            .unwrap()
            .active_signing_sessions()
            .filter(|session| session.key_id == key_id)
            .collect()
    }

    #[frb(sync)]
    pub fn unbroadcasted_txs(
        &self,
        s_wallet: &SuperWallet,
        master_appkey: MasterAppkey,
    ) -> Vec<UnbroadcastedTx> {
        let key_id = master_appkey.key_id();
        let coord = self.0.coordinator.lock().unwrap();

        let s_wallet = &mut *s_wallet.inner.lock().unwrap();
        let canonical_txids = s_wallet
            .list_transactions(master_appkey)
            .into_iter()
            .map(|tx| tx.txid)
            .collect::<HashSet<bitcoin::Txid>>();

        let unsigned_txs = coord
            .active_signing_sessions()
            .filter(|session| session.key_id == key_id)
            .filter_map(|session| {
                let sign_task = &session.init.group_request.sign_task;
                match sign_task {
                    WireSignTask::BitcoinTransaction(tx_temp) => {
                        let tx = Transaction::from_template(tx_temp);
                        let session_id = session.session_id();
                        Some(UnbroadcastedTx {
                            tx,
                            session_id,
                            active_session: Some(session),
                        })
                    }
                    _ => None,
                }
            });

        let unbroadcasted_txs = coord
            .finished_signing_sessions()
            .iter()
            .filter(|(_, session)| session.key_id == key_id)
            .filter_map(
                |(&session_id, session)| match &session.init.group_request.sign_task {
                    WireSignTask::BitcoinTransaction(tx_temp) => {
                        let mut tx = Transaction::from_template(tx_temp);
                        tx.fill_signatures(&session.signatures);
                        Some(UnbroadcastedTx {
                            tx,
                            session_id,
                            active_session: None,
                        })
                    }
                    _ => None,
                },
            );

        unsigned_txs
            .chain(unbroadcasted_txs)
            .filter(move |uncanonical_tx| {
                let txid = uncanonical_tx.tx.raw_txid();
                !canonical_txids.contains(&txid)
            })
            .collect()
    }

    pub fn cancel_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        let session = {
            let mut db = self.0.db.lock().unwrap();
            event!(
                Level::INFO,
                ssid = ssid.to_string(),
                "canceling sign session"
            );
            let mut coord = self.0.coordinator.lock().unwrap();
            let session = coord.active_signing_sessions_by_ssid().get(&ssid).cloned();
            coord.staged_mutate(&mut *db, |coordinator| {
                coordinator.cancel_sign_session(ssid);
                Ok(())
            })?;
            session
        };
        if let Some(session) = session {
            self.0.emit_signing_signal(session.key_id);
        }
        Ok(())
    }

    pub fn forget_finished_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        let deleted_session = {
            let mut db = self.0.db.lock().unwrap();
            event!(
                Level::INFO,
                ssid = ssid.to_string(),
                "forgetting finished sign session"
            );
            let mut coord = self.0.coordinator.lock().unwrap();
            coord.staged_mutate(&mut *db, |coordinator| {
                Ok(coordinator.forget_finished_sign_session(ssid))
            })?
        };
        if let Some(session) = deleted_session {
            self.0.emit_signing_signal(session.key_id);
        }
        Ok(())
    }

    pub fn sub_signing_session_signals(&self, key_id: KeyId, sink: StreamSink<()>) {
        // Emit an initial signal immediately so that subscribers (especially
        // BehaviorSubjects on the Dart side) get an initial value.
        let wrapped = SinkWrap(sink);
        frostsnap_coordinator::Sink::send(&wrapped, ());
        self.0
            .signing_session_signals
            .lock()
            .unwrap()
            .insert(key_id, Box::new(wrapped));
    }

    pub fn reserve_nonces(
        &self,
        id: RemoteSignSessionId,
        access_structure_ref: AccessStructureRef,
        sign_task: WireSignTask,
        device_id: DeviceId,
    ) -> Result<ParticipantBinonces> {
        let mut db = self.0.db.lock().unwrap();
        let mut coord = self.0.coordinator.lock().unwrap();
        coord.staged_mutate(&mut *db, |coordinator| {
            let offer =
                coordinator.offer_to_sign(id, access_structure_ref, sign_task, device_id)?;
            Ok(offer.participant_binonces)
        })
    }

    pub fn cancel_remote_sign_session(&self, id: RemoteSignSessionId) -> Result<()> {
        let mut db = self.0.db.lock().unwrap();
        let mut coord = self.0.coordinator.lock().unwrap();
        coord.staged_mutate(&mut *db, |coordinator| {
            coordinator.cancel_remote_sign_session(id);
            Ok(())
        })
    }

    /// Returns every completed signature share cached under this
    /// `RemoteSignSessionId`, paired with the device that produced it.
    #[frb(sync)]
    pub fn get_completed_signature_shares(
        &self,
        id: RemoteSignSessionId,
    ) -> Vec<(DeviceId, ParticipantSignatureShares)> {
        self.0
            .coordinator
            .lock()
            .unwrap()
            .get_completed_signature_shares(id)
            .into_iter()
            .collect()
    }

    #[frb(sync)]
    pub fn can_sign_with_nonce_reservation(
        &self,
        all_binonces: Vec<ParticipantBinonces>,
        id: RemoteSignSessionId,
        device_id: DeviceId,
    ) -> bool {
        self.0
            .coordinator
            .lock()
            .unwrap()
            .can_sign_with_nonce_reservation(&all_binonces, id, device_id)
    }

    #[frb(sync)]
    pub fn get_device_signature_shares(
        &self,
        session_id: SignSessionId,
        device_id: DeviceId,
    ) -> Option<ParticipantSignatureShares> {
        self.0
            .coordinator
            .lock()
            .unwrap()
            .get_device_signature_shares(session_id, device_id)
    }
}

#[derive(Clone, Debug)]
#[frb(type_64bit_int)]
pub struct EffectOfTx {
    pub net_value: i64,
    pub fee: u64,
    pub feerate: Option<f64>,
    pub foreign_receiving_addresses: Vec<(String, u64)>,
}
