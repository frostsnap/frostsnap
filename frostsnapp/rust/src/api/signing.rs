use super::super_wallet::SuperWallet;
use super::{
    bitcoin::{Psbt, RTransaction, Transaction, TxOutInfo},
    coordinator::Coordinator,
};
use crate::{
    frb_generated::StreamSink,
    sink_wrap::{report_start_failure, SinkWrap},
};
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::signing::SigningState;
pub use frostsnap_core::bitcoin_transaction::{ScopedTo, TransactionTemplate};
pub use frostsnap_core::coordinator::ActiveSignSession;
pub use frostsnap_core::coordinator::{SignSessionProgress, StartSign};
use frostsnap_core::MasterAppkey;
use frostsnap_core::{
    message::EncodedSignature, AccessStructureRef, DeviceId, KeyId, SignSessionId, SymmetricKey,
    WireSignTask,
};
use std::collections::HashSet;
use tracing::{event, Level};

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
            needed_from: session_init.nonces.keys().copied().collect(),
            finished_signatures: None,
            aborted: None,
            connected_but_need_request: Default::default(),
        };

        state
    }

    #[frb(sync)]
    fn access_structure_ref(&self) -> AccessStructureRef {
        ActiveSignSession::access_structure_ref(self)
    }
}

#[derive(Clone, Debug)]
pub struct UnsignedTx {
    /// The form that will be sent. Held unscoped because that is what goes on the wire, and
    /// there is deliberately no way back from a scoped one.
    ///
    /// Private so [`Self::new`] is the only way to get one, which is what makes its check an
    /// invariant rather than a habit of the two callers. It also leaves frb no accessor to
    /// generate, which it could not do for a second instantiation of a generic anyway.
    template_tx: TransactionTemplate,
    /// Whose transaction this is. Every question about ownership needs it.
    pub master_appkey: MasterAppkey,
}

impl UnsignedTx {
    /// Fails when no input belongs to `master_appkey`.
    ///
    /// Every reader below answers *for* that key without re-checking, so a template with
    /// nothing of its own would report a balance and a recipient list for a transaction the
    /// key has no part in. `TransactionTemplate::from_psbt` rejects a PSBT owned by none of
    /// the keys it was given, which is a weaker statement as soon as it is given more
    /// than one.
    #[frb(ignore)]
    pub(crate) fn new(
        template_tx: TransactionTemplate,
        master_appkey: MasterAppkey,
    ) -> Option<Self> {
        template_tx
            .as_seen_by(master_appkey)
            .has_any_inputs_to_sign()
            .then_some(Self {
                template_tx,
                master_appkey,
            })
    }

    /// This transaction as its own key sees it. Cheap enough for the handful of UI-rate
    /// reads below, and it keeps the sendable form the only one stored.
    #[frb(ignore)]
    fn scoped(&self) -> TransactionTemplate<ScopedTo> {
        self.template_tx.as_seen_by(self.master_appkey)
    }

    /// Whether we alone can produce something broadcastable.
    ///
    /// `false` for a PSBT holding inputs that are not ours: our signatures go back into the
    /// PSBT for whoever else must sign, and offering to broadcast would produce a transaction
    /// the network rejects.
    #[frb(sync)]
    pub fn can_broadcast(&self) -> bool {
        self.scoped().owns_every_input()
    }

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
        self.scoped().feerate()
    }

    #[frb(sync)]
    pub fn details(&self) -> Transaction {
        Transaction::from_template(&self.scoped())
    }

    /// The transaction with these signatures witnessed onto the inputs they were produced for.
    ///
    /// Lives here rather than on `Transaction` because it needs the template, and only a
    /// transaction that came from a signing session has one. `signatures_by_input_index` on
    /// the scoped template stays the only thing deciding which signature belongs where.
    pub fn with_signatures(&self, signatures: Vec<EncodedSignature>) -> Result<RTransaction> {
        Ok(self.scoped().to_signed_rust_bitcoin_tx(&signatures)?)
    }

    /// `None` when the signatures cannot be placed — the list does not match the inputs this
    /// key owns, or the PSBT has fewer inputs than the template it was signed against.
    #[frb(sync)]
    pub fn attach_signatures_to_psbt(
        &self,
        signatures: Vec<EncodedSignature>,
        psbt: &Psbt,
    ) -> Option<Psbt> {
        match self.scoped().attach_signatures_to_psbt(&signatures, psbt) {
            Ok(psbt) => Some(psbt),
            Err(e) => {
                event!(Level::ERROR, error = e.to_string(), "couldn't sign PSBT");
                None
            }
        }
    }

    #[frb(sync)]
    pub fn complete(&self, signatures: Vec<EncodedSignature>) -> Result<SignedTx> {
        Ok(SignedTx {
            signed_tx: self.scoped().to_signed_rust_bitcoin_tx(&signatures)?,
            unsigned_tx: self.clone(),
        })
    }

    /// Takes no key: which key this is about is a property of the transaction, not of the
    /// question, and a parameter would let a caller ask about one it is not for.
    #[frb(sync)]
    /// Takes no network: an address is a rendering, and rendering belongs to whoever displays.
    pub fn effect(&self) -> Result<EffectOfTx> {
        let scoped = self.scoped();
        let fee = self
            .template_tx
            .fee()
            .ok_or(anyhow!("invalid transaction"))?;

        let foreign_outputs = self
            .details()
            .recipients()
            .into_iter()
            .filter(|output| !output.is_mine)
            .collect();

        Ok(EffectOfTx {
            net_value: scoped.our_net_value(),
            fee,
            feerate: scoped.feerate(),
            foreign_outputs,
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
    pub fn effect(&self) -> Result<EffectOfTx> {
        self.unsigned_tx.effect()
    }
}

/// A signing session that is over, and what came out of it.
///
/// One value from one lookup: a transaction and the signatures that completed it cannot be
/// fetched separately and then disagree about which session they came from.
#[derive(Debug, Clone)]
pub struct FinishedSigning {
    pub unsigned_tx: UnsignedTx,
    pub signatures: Vec<EncodedSignature>,
}

impl Coordinator {
    pub fn start_signing(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: Vec<DeviceId>,
        message: String,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        report_start_failure(sink, |sink| {
            self.0.start_signing(
                access_structure_ref,
                devices.into_iter().collect(),
                WireSignTask::Test { message },
                sink,
            )
        })
    }

    /// Borrows rather than takes: frb disposes the Dart handle for a value it moves, and the
    /// caller still needs this one — the same transaction answers for the review screen and
    /// receives the signatures when they arrive. Nothing here wants ownership; the template is
    /// cloned onto the wire either way.
    pub fn start_signing_tx(
        &self,
        access_structure_ref: AccessStructureRef,
        unsigned_tx: &UnsignedTx,
        devices: Vec<DeviceId>,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        report_start_failure(sink, |sink| {
            self.0.start_signing(
                access_structure_ref,
                devices.into_iter().collect(),
                WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.clone()),
                sink,
            )
        })
    }

    #[frb(sync)]
    pub fn nonces_available(&self, id: DeviceId) -> u32 {
        self.0.nonces_available(id)
    }

    pub fn try_restore_signing_session(
        &self,
        session_id: SignSessionId,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        report_start_failure(sink, |sink| {
            self.0.try_restore_signing_session(session_id, sink)
        })
    }

    #[frb(sync)]
    pub fn active_signing_session(&self, session_id: SignSessionId) -> Option<ActiveSignSession> {
        self.0
            .inner()
            .active_signing_sessions_by_ssid()
            .get(&session_id)
            .cloned()
    }

    /// What a finished session produced, as `master_appkey` sees it.
    ///
    /// The signatures are durable state here, not something a screen can only learn by
    /// watching a stream — which is why a session reopened after signing had none: it
    /// subscribes to nothing, correctly, and was reading stream state for a stored fact.
    ///
    /// `None` when there is no finished session with that id, or it was not signing a bitcoin
    /// transaction.
    #[frb(sync)]
    pub fn finished_signing_for_session(
        &self,
        session_id: SignSessionId,
        master_appkey: MasterAppkey,
    ) -> Option<FinishedSigning> {
        let coord = self.0.inner();
        let session = coord.finished_signing_sessions().get(&session_id)?;
        match &session.init.group_request.sign_task {
            WireSignTask::BitcoinTransaction(tx_temp) => {
                // A session with no signatures has not finished anything. Returning it would
                // hand a screen an empty list that reads as "ready to broadcast" and fails at
                // the count check, which is the failure this whole plan is about.
                if session.signatures.is_empty() {
                    return None;
                }
                Some(FinishedSigning {
                    unsigned_tx: UnsignedTx::new(tx_temp.clone(), master_appkey)?,
                    signatures: session.signatures.clone(),
                })
            }
            _ => None,
        }
    }

    /// The transaction an in-flight session is signing, as `master_appkey` sees it.
    ///
    /// A session reopened from its id recovers this the same way it recovers its devices and
    /// access structure: from the session. The alternative is the transaction on screen
    /// carrying the template, which is what made `Transaction` two types at once.
    ///
    /// Looks in finished sessions as well as active ones: a transaction waiting to be
    /// broadcast has a session that is over, and that screen needs the transaction most of
    /// all — it is the only state that can broadcast.
    ///
    /// `None` means there is no such session either way, or it is not signing a bitcoin
    /// transaction. Missing data, not a mode — a session can end or be forgotten between a
    /// caller's check and this call.
    #[frb(sync)]
    pub fn unsigned_tx_for_session(
        &self,
        session_id: SignSessionId,
        master_appkey: MasterAppkey,
    ) -> Option<UnsignedTx> {
        let coord = self.0.inner();
        let sign_task = coord
            .active_signing_sessions_by_ssid()
            .get(&session_id)
            .map(|session| session.init.group_request.sign_task.clone())
            .or_else(|| {
                coord
                    .finished_signing_sessions()
                    .get(&session_id)
                    .map(|session| session.init.group_request.sign_task.clone())
            })?;
        match sign_task {
            WireSignTask::BitcoinTransaction(tx_temp) => UnsignedTx::new(tx_temp, master_appkey),
            _ => None,
        }
    }

    #[frb(sync)]
    pub fn active_signing_sessions(&self, key_id: KeyId) -> Vec<ActiveSignSession> {
        self.0
            .inner()
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
        let coord = self.0.inner();

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
                        let tx = Transaction::from_template(&tx_temp.as_seen_by(master_appkey));
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
                        // Showing an unbroadcastable transaction is worse than omitting it:
                        // its witnesses would be on inputs that did not produce them.
                        let tx = match Transaction::signed_from_template(
                            &tx_temp.as_seen_by(master_appkey),
                            &session.signatures,
                        ) {
                            Ok(tx) => tx,
                            Err(e) => {
                                event!(
                                    Level::ERROR,
                                    session = session_id.to_string(),
                                    error = e.to_string(),
                                    "signatures don't match the transaction they signed"
                                );
                                return None;
                            }
                        };
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

    #[frb(sync)]
    pub fn request_device_sign(
        &self,
        device_id: DeviceId,
        session_id: SignSessionId,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        self.0
            .request_device_sign(device_id, session_id, encryption_key)
    }

    pub fn cancel_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        self.0.cancel_sign_session(ssid)
    }

    pub fn forget_finished_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        self.0.forget_finished_sign_session(ssid)
    }

    pub fn sub_signing_session_signals(&self, key_id: KeyId, sink: StreamSink<()>) {
        self.0.sub_signing_session_signals(key_id, SinkWrap(sink))
    }
}

#[derive(Clone, Debug)]
#[frb(type_64bit_int)]
pub struct EffectOfTx {
    pub net_value: i64,
    pub fee: u64,
    pub feerate: Option<f64>,
    /// One entry per output that is not ours, gross. `TxOutInfo::address` already answers
    /// `Option`, so a script with no address form is a row the caller renders rather than a
    /// panic — and one entry per output means two payments to one address stay two rows.
    pub foreign_outputs: Vec<TxOutInfo>,
}
