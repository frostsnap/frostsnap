use super::super_wallet::SuperWallet;
use super::{
    bitcoin::{BitcoinNetwork, RTransaction, Transaction},
    coordinator::Coordinator,
};
use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
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
    /// key has no part in. `psbt_to_tx_template` rejects a PSBT owned by none of the keys it
    /// was given, which is a weaker statement as soon as it is given more than one.
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
    pub fn effect(&self, network: BitcoinNetwork) -> Result<EffectOfTx> {
        let scoped = self.scoped();
        let fee = self
            .template_tx
            .fee()
            .ok_or(anyhow!("invalid transaction"))?;

        let foreign_receiving_addresses = scoped
            .foreign_net_values()
            .into_iter()
            .filter(|(_, value)| *value > 0)
            .map(|(spk, value)| {
                (
                    bitcoin::Address::from_script(spk.as_script(), network)
                        .expect("will have address form")
                        .to_string(),
                    value as u64,
                )
            })
            .collect();

        Ok(EffectOfTx {
            net_value: scoped.our_net_value(),
            fee,
            feerate: scoped.feerate(),
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
    pub fn effect(&self, network: BitcoinNetwork) -> Result<EffectOfTx> {
        self.unsigned_tx.effect(network)
    }
}

impl Coordinator {
    pub fn start_signing(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: Vec<DeviceId>,
        message: String,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::Test { message },
            SinkWrap(sink),
        )?;
        Ok(())
    }

    pub fn start_signing_tx(
        &self,
        access_structure_ref: AccessStructureRef,
        unsigned_tx: UnsignedTx,
        devices: Vec<DeviceId>,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.clone()),
            SinkWrap(sink),
        )?;
        Ok(())
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
        self.0
            .try_restore_signing_session(session_id, SinkWrap(sink))
    }

    #[frb(sync)]
    pub fn active_signing_session(&self, session_id: SignSessionId) -> Option<ActiveSignSession> {
        self.0
            .inner()
            .active_signing_sessions_by_ssid()
            .get(&session_id)
            .cloned()
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
                        let mut tx = Transaction::from_template(&tx_temp.as_seen_by(master_appkey));
                        // Showing an unbroadcastable transaction is worse than omitting it:
                        // its witnesses would be on inputs that did not produce them.
                        if let Err(e) = tx.fill_signatures(&session.signatures) {
                            event!(
                                Level::ERROR,
                                session = session_id.to_string(),
                                error = e.to_string(),
                                "signatures don't match the transaction they signed"
                            );
                            return None;
                        }
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
    pub foreign_receiving_addresses: Vec<(String, u64)>,
}
