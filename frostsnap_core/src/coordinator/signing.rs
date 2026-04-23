use super::*;
use crate::{
    coord_nonces::NotEnoughNonces,
    fail,
    message::{DeviceSignReq, EncodedSignature, GroupSignReq, RequestSign},
    nonce_stream::{CoordNonceStreamState, NonceStreamSegment},
    tweak::Xpub,
    AccessStructureRef, DeviceId, KeyId, Kind, MasterAppkey, SignItem, SignSessionId,
    SignTaskError,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::fmt;
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::{
    frost::{self, CoordinatorSignSession, Frost, SharedKey, SignatureShare},
    Schnorr, Signature,
};
use std::collections::HashSet;

// ============================================================================
// Mutations
// ============================================================================

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, KindDerive)]
pub enum SigningMutation {
    NewNonces {
        device_id: DeviceId,
        nonce_segment: NonceStreamSegment,
    },
    NewSigningSession(ActiveSignSession),
    SentSignReq {
        session_id: SignSessionId,
        device_id: DeviceId,
    },
    GotSignatureSharesFromDevice {
        session_id: SignSessionId,
        device_id: DeviceId,
        signature_shares: Vec<SignatureShare>,
    },
    CloseSignSession {
        session_id: SignSessionId,
        finished: Option<Vec<EncodedSignature>>,
    },
    ForgetFinishedSignSession {
        session_id: SignSessionId,
    },
}

impl SigningMutation {
    pub fn tied_to_key(&self, coord: &FrostCoordinator) -> Option<KeyId> {
        match self {
            SigningMutation::NewNonces { .. } => None,
            SigningMutation::NewSigningSession(active_sign_session) => {
                Some(active_sign_session.key_id)
            }
            SigningMutation::SentSignReq { session_id, .. }
            | SigningMutation::GotSignatureSharesFromDevice { session_id, .. }
            | SigningMutation::CloseSignSession { session_id, .. }
            | SigningMutation::ForgetFinishedSignSession { session_id } => {
                Some(coord.get_sign_session(*session_id)?.key_id())
            }
        }
    }
}

// ============================================================================
// Session types
// ============================================================================

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SignSessionProgress {
    pub sign_item: SignItem,
    pub sign_session: CoordinatorSignSession,
    pub signature_shares: BTreeMap<DeviceId, SignatureShare>,
    pub app_shared_key: Xpub<SharedKey>,
}

impl SignSessionProgress {
    pub fn new<NG>(
        frost: &Frost<sha2::Sha256, NG>,
        app_shared_key: Xpub<SharedKey>,
        sign_item: SignItem,
        nonces: BTreeMap<frost::ShareIndex, frost::Nonce>,
        rng: &mut impl rand_core::RngCore,
    ) -> Self {
        let tweaked_key = sign_item.app_tweak.derive_xonly_key(&app_shared_key);
        let sign_session = frost.randomized_coordinator_sign_session(
            &tweaked_key,
            nonces,
            sign_item.schnorr_fun_message(),
            rng,
        );

        Self {
            sign_item,
            sign_session,
            signature_shares: Default::default(),
            app_shared_key,
        }
    }

    /// Create without randomization - all coordinators with same inputs get same session.
    pub fn new_deterministic<NG>(
        frost: &Frost<sha2::Sha256, NG>,
        app_shared_key: Xpub<SharedKey>,
        sign_item: SignItem,
        nonces: BTreeMap<frost::ShareIndex, frost::Nonce>,
    ) -> Self {
        let tweaked_key = sign_item.app_tweak.derive_xonly_key(&app_shared_key);
        let sign_session =
            frost.coordinator_sign_session(&tweaked_key, nonces, sign_item.schnorr_fun_message());

        Self {
            sign_item,
            sign_session,
            signature_shares: Default::default(),
            app_shared_key,
        }
    }

    pub fn received_from(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.signature_shares.keys().cloned()
    }

    pub fn tweaked_frost_key(&self) -> SharedKey<EvenY> {
        self.sign_item
            .app_tweak
            .derive_xonly_key(&self.app_shared_key)
    }

    pub fn verify_final_signature<NG>(
        &self,
        schnorr: &Schnorr<sha2::Sha256, NG>,
        signature: &Signature,
    ) -> bool {
        let master_appkey = MasterAppkey::from_xpub_unchecked(&self.app_shared_key);
        self.sign_item
            .verify_final_signature(schnorr, master_appkey, signature)
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct ActiveSignSession {
    pub progress: Vec<SignSessionProgress>,
    pub init: StartSign,
    pub key_id: KeyId,
    pub sent_req_to_device: HashSet<DeviceId>,
}

impl ActiveSignSession {
    pub fn access_structure_ref(&self) -> AccessStructureRef {
        AccessStructureRef {
            key_id: self.key_id,
            access_structure_id: self.init.group_request.access_structure_id,
        }
    }

    pub fn received_from(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.progress
            .first()
            .into_iter()
            .flat_map(|p| p.received_from())
    }

    pub fn has_received_from(&self, device_id: DeviceId) -> bool {
        self.progress
            .first()
            .map(|p| p.signature_shares.contains_key(&device_id))
            .unwrap_or(false)
    }

    pub fn session_id(&self) -> SignSessionId {
        self.init.group_request.session_id()
    }

    pub fn is_finished(&self) -> bool {
        self.progress
            .iter()
            .all(|session| session.signature_shares.len() == self.init.group_request.parties.len())
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct FinishedSignSession {
    pub init: StartSign,
    pub signatures: Vec<EncodedSignature>,
    pub key_id: KeyId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignSession {
    Active(ActiveSignSession),
    Finished(FinishedSignSession),
}

impl SignSession {
    pub fn key_id(&self) -> KeyId {
        match self {
            SignSession::Active(active) => active.key_id,
            SignSession::Finished(finished) => finished.key_id,
        }
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct StartSign {
    /// Nonce stream allocations for devices whose nonces we manage locally.
    /// We need to consume these streams when the session completes.
    pub local_nonces: BTreeMap<DeviceId, CoordNonceStreamState>,
    pub group_request: GroupSignReq,
}

pub use super::remote_signing::{
    RemoteSignSession, RemoteSignSessionId, RemoteSignStatus, SignOffer,
};

/// Binonces for a participant (local or remote).
#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct ParticipantBinonces {
    pub share_index: frost::ShareIndex,
    pub binonces: Vec<schnorr_fun::binonce::Nonce>,
}

impl GroupSignReq {
    /// Build a `GroupSignReq` from collected participant binonces.
    pub fn from_binonces(
        sign_task: crate::WireSignTask,
        access_structure_id: crate::AccessStructureId,
        all_binonces: &[ParticipantBinonces],
    ) -> Self {
        use schnorr_fun::binonce::Nonce as Binonce;

        let n_signatures = all_binonces.first().map(|b| b.binonces.len()).unwrap_or(0);
        let agg_nonces: Vec<_> = (0..n_signatures)
            .map(|i| Binonce::aggregate(all_binonces.iter().map(|b| b.binonces[i])))
            .collect();

        GroupSignReq {
            sign_task,
            parties: all_binonces.iter().map(|b| b.share_index).collect(),
            agg_nonces,
            access_structure_id,
        }
    }
}

/// Signature shares from a participant (local or remote).
#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct ParticipantSignatureShares {
    pub share_index: frost::ShareIndex,
    pub signature_shares: Vec<SignatureShare>,
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Clone)]
pub enum StartSignError {
    UnknownKey {
        key_id: KeyId,
    },
    DeviceNotPartOfKey {
        device_id: DeviceId,
    },
    NotEnoughDevicesSelected {
        selected: usize,
        threshold: u16,
    },
    CantSignInState {
        in_state: &'static str,
    },
    NotEnoughNoncesForDevice(NotEnoughNonces),
    SignTask(SignTaskError),
    NoSuchAccessStructure,
    CouldntDecryptRootKey,
    DeviceNotLocalSigner {
        device_id: DeviceId,
    },
    NoSuchRemoteSignSession,
    NoSuchSignSession {
        session_id: SignSessionId,
    },
    /// A second `offer_to_sign` for the same `(RemoteSignSessionId, DeviceId)`
    /// supplied different `access_structure_ref` or `sign_task` than the
    /// first. Remote sessions commit to those values at offer time.
    ConflictingRemoteSignSession,
}

impl fmt::Display for StartSignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartSignError::NotEnoughDevicesSelected {
                selected,
                threshold,
            } => {
                write!(
                    f,
                    "Need more than {selected} signers for threshold {threshold}",
                )
            }
            StartSignError::CantSignInState { in_state } => {
                write!(f, "Can't sign in state {in_state}")
            }
            StartSignError::NotEnoughNoncesForDevice(not_enough_nonces) => not_enough_nonces.fmt(f),
            StartSignError::DeviceNotPartOfKey { device_id } => {
                write!(
                    f,
                    "Don't know the share index for device that was part of sign request. ID: {device_id}",
                )
            }
            StartSignError::UnknownKey { key_id } => write!(
                f,
                "device does not have key is was asked to sign with, id: {key_id}"
            ),
            StartSignError::SignTask(error) => {
                write!(f, "{error}")
            }
            StartSignError::NoSuchAccessStructure => write!(
                f,
                "the access structure you wanted to sign with did not exist"
            ),
            StartSignError::CouldntDecryptRootKey => write!(f, "the decryption key did not"),
            StartSignError::DeviceNotLocalSigner { device_id } => write!(
                f,
                "device {device_id} did not provide nonces for this session"
            ),
            StartSignError::NoSuchRemoteSignSession => {
                write!(f, "no open remote sign session found with that id")
            }
            StartSignError::NoSuchSignSession { session_id } => {
                write!(f, "no active sign session found with id {session_id}")
            }
            StartSignError::ConflictingRemoteSignSession => write!(
                f,
                "a remote sign session with this id was offered with different access structure or sign task"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StartSignError {}

pub use super::remote_signing::{combine_signatures, CombineSignatureError};

// ============================================================================
// State
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub(super) active_signing_sessions: BTreeMap<SignSessionId, ActiveSignSession>,
    pub(super) active_sign_session_order: Vec<SignSessionId>,
    pub(super) finished_signing_sessions: BTreeMap<SignSessionId, FinishedSignSession>,
    pub(super) nonce_cache: crate::coord_nonces::NonceCache,
}

impl State {
    pub fn get_session(&self, id: &SignSessionId) -> Option<&ActiveSignSession> {
        self.active_signing_sessions.get(id)
    }

    pub fn get_session_mut(&mut self, id: &SignSessionId) -> Option<&mut ActiveSignSession> {
        self.active_signing_sessions.get_mut(id)
    }

    pub fn apply_mutation_signing(&mut self, mutation: SigningMutation) -> Option<SigningMutation> {
        match mutation {
            SigningMutation::NewNonces {
                device_id,
                ref nonce_segment,
            } => {
                match self
                    .nonce_cache
                    .extend_segment(device_id, nonce_segment.clone())
                {
                    Ok(changed) => {
                        if !changed {
                            return None;
                        }
                    }
                    Err(e) => fail!("failed to extend nonce segment: {e}"),
                }
            }
            SigningMutation::NewSigningSession(ref signing_session_state) => {
                let ssid = signing_session_state.init.group_request.session_id();
                self.active_signing_sessions
                    .insert(ssid, signing_session_state.clone());
                self.active_sign_session_order.push(ssid);
            }
            SigningMutation::GotSignatureSharesFromDevice {
                session_id,
                device_id,
                ref signature_shares,
            } => {
                if let Some(session_state) = self.active_signing_sessions.get_mut(&session_id) {
                    for (progress, share) in session_state.progress.iter_mut().zip(signature_shares)
                    {
                        progress.signature_shares.insert(device_id, *share);
                    }
                }
            }
            SigningMutation::SentSignReq {
                session_id,
                device_id,
            } => {
                if !self
                    .active_signing_sessions
                    .get_mut(&session_id)?
                    .sent_req_to_device
                    .insert(device_id)
                {
                    return None;
                }
            }
            SigningMutation::CloseSignSession {
                session_id,
                ref finished,
            } => {
                if let Some((index, _)) = self
                    .active_sign_session_order
                    .iter()
                    .enumerate()
                    .find(|(_, ssid)| **ssid == session_id)
                {
                    self.active_sign_session_order.remove(index);
                    let session_state = self
                        .active_signing_sessions
                        .remove(&session_id)
                        .expect("it existed in the order");
                    let n_sigs = session_state.init.group_request.n_signatures();
                    for (device_id, nonce_segment) in &session_state.init.local_nonces {
                        if session_state.sent_req_to_device.contains(device_id) {
                            let consume_to = nonce_segment
                                .index
                                .checked_add(n_sigs as _)
                                .expect("no overflow");

                            self.nonce_cache.consume(
                                *device_id,
                                nonce_segment.stream_id,
                                consume_to,
                            );
                        }
                    }
                    if let Some(signatures) = finished {
                        self.finished_signing_sessions.insert(
                            session_id,
                            FinishedSignSession {
                                init: session_state.init,
                                signatures: signatures.clone(),
                                key_id: session_state.key_id,
                            },
                        );
                    }
                } else {
                    return None;
                }
            }
            SigningMutation::ForgetFinishedSignSession { session_id } => {
                self.finished_signing_sessions.remove(&session_id);
            }
        }

        Some(mutation)
    }

    pub fn clear_up_key_deletion(&mut self, key_id: KeyId) {
        let sessions_to_delete: Vec<_> = self
            .active_signing_sessions
            .iter()
            .filter(|(_, session)| session.key_id == key_id)
            .map(|(&session_id, _)| session_id)
            .collect();

        for session_id in sessions_to_delete {
            if let Some((index, _)) = self
                .active_sign_session_order
                .iter()
                .enumerate()
                .find(|(_, ssid)| **ssid == session_id)
            {
                self.active_sign_session_order.remove(index);
            }
            self.active_signing_sessions.remove(&session_id);
        }

        self.finished_signing_sessions
            .retain(|_, session| session.key_id != key_id);
    }

    pub fn all_used_nonce_streams(&self) -> BTreeSet<crate::nonce_stream::NonceStreamId> {
        self.active_signing_sessions
            .values()
            .flat_map(|session| {
                session
                    .init
                    .local_nonces
                    .values()
                    .map(|device_nonces| device_nonces.stream_id)
            })
            .collect()
    }
}

// ============================================================================
// FrostCoordinator impl
// ============================================================================

impl FrostCoordinator {
    pub fn start_sign(
        &mut self,
        access_structure_ref: AccessStructureRef,
        sign_task: WireSignTask,
        signing_devices: &BTreeSet<DeviceId>,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<SignSessionId, StartSignError> {
        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;

        let key_data = self
            .keys
            .get(&key_id)
            .ok_or(StartSignError::UnknownKey { key_id })?
            .clone();

        let complete_key = key_data.complete_key;

        let access_structure = complete_key
            .access_structures
            .get(&access_structure_id)
            .ok_or(StartSignError::NoSuchAccessStructure)?;

        for device in signing_devices {
            if !access_structure.device_to_share_index.contains_key(device) {
                return Err(StartSignError::DeviceNotPartOfKey { device_id: *device });
            }
        }

        let app_shared_key = access_structure.app_shared_key().clone();

        let selected = signing_devices.len();
        if selected < access_structure.threshold() as usize {
            return Err(StartSignError::NotEnoughDevicesSelected {
                selected,
                threshold: access_structure.threshold(),
            });
        }

        let checked_sign_task = sign_task
            .clone()
            .check(complete_key.master_appkey, key_data.purpose)
            .map_err(StartSignError::SignTask)?;

        let sign_items = checked_sign_task.sign_items();
        let n_signatures = sign_items.len();

        let nonces_by_device = self
            .signing
            .nonce_cache
            .new_signing_session(
                signing_devices,
                n_signatures,
                &self.signing.all_used_nonce_streams(),
            )
            .map_err(StartSignError::NotEnoughNoncesForDevice)?;

        let nonces_by_party = nonces_by_device
            .iter()
            .map(|(device_id, nonce_segment)| {
                (
                    *access_structure
                        .device_to_share_index
                        .get(device_id)
                        .expect("checked already"),
                    nonce_segment.segment.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let frost = frost::new_without_nonce_generation::<Sha256>();
        let sessions = sign_items
            .iter()
            .enumerate()
            .map(|(i, sign_item)| {
                let indexed_nonces = nonces_by_party
                    .iter()
                    .map(|(party_index, nonce_segment)| (*party_index, nonce_segment.nonces[i]))
                    .collect();

                SignSessionProgress::new(
                    &frost,
                    app_shared_key.clone(),
                    sign_item.clone(),
                    indexed_nonces,
                    rng,
                )
            })
            .collect::<Vec<_>>();

        let group_request = GroupSignReq {
            sign_task,
            parties: nonces_by_party.keys().cloned().collect(),
            agg_nonces: sessions
                .iter()
                .map(|session| session.sign_session.agg_binonce())
                .collect(),
            access_structure_id,
        };
        let session_id = group_request.session_id();

        let device_requests = nonces_by_device
            .into_iter()
            .map(|(device, nonce_segment)| (device, nonce_segment.coord_nonce_state()))
            .collect();

        let start_sign = StartSign {
            local_nonces: device_requests,
            group_request,
        };

        let local_session = ActiveSignSession {
            progress: sessions,
            init: start_sign.clone(),
            key_id,
            sent_req_to_device: Default::default(),
        };

        self.mutate(Mutation::Signing(SigningMutation::NewSigningSession(
            local_session,
        )));

        Ok(session_id)
    }

    pub fn request_device_sign(
        &mut self,
        session_id: SignSessionId,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> Result<RequestDeviceSign, StartSignError> {
        let active_sign_session = self
            .signing
            .get_session(&session_id)
            .ok_or(StartSignError::NoSuchSignSession { session_id })?;

        let nonces_for_device = *active_sign_session
            .init
            .local_nonces
            .get(&device_id)
            .ok_or(StartSignError::DeviceNotLocalSigner { device_id })?;

        let key_id = active_sign_session.key_id;
        let group_sign_req = active_sign_session.init.group_request.clone();

        let complete_key = &self
            .keys
            .get(&key_id)
            .ok_or(StartSignError::UnknownKey { key_id })?
            .complete_key;
        let (rootkey, coord_share_decryption_contrib) = complete_key
            .coord_share_decryption_contrib(
                group_sign_req.access_structure_id,
                device_id,
                encryption_key,
            )
            .ok_or(StartSignError::CouldntDecryptRootKey)?;

        self.mutate(Mutation::Signing(SigningMutation::SentSignReq {
            device_id,
            session_id,
        }));

        Ok(RequestDeviceSign {
            device_id,
            request_sign: RequestSign {
                group_sign_req,
                device_sign_req: DeviceSignReq {
                    nonces: nonces_for_device,
                    rootkey,
                    coord_share_decryption_contrib,
                },
            },
        })
    }

    pub fn complete_sign_session(
        &mut self,
        session_id: SignSessionId,
    ) -> Option<Vec<EncodedSignature>> {
        let sign_state = self.signing.get_session(&session_id)?;

        if sign_state.is_finished() {
            let sessions = &sign_state.progress;

            let signatures: Vec<_> = sessions
                .iter()
                .map(|session_progress| {
                    let sig = session_progress.sign_session.combine_signature_shares(
                        session_progress
                            .signature_shares
                            .iter()
                            .map(|(_, &share)| share),
                    );

                    assert!(
                        session_progress.verify_final_signature(
                            &Schnorr::<sha2::Sha256, _>::verify_only(),
                            &sig,
                        ),
                        "we have verified the signature shares so combined should be correct"
                    );

                    sig
                })
                .map(EncodedSignature::new)
                .collect();

            self.mutate(Mutation::Signing(SigningMutation::CloseSignSession {
                session_id,
                finished: Some(signatures.clone()),
            }));

            Some(signatures)
        } else {
            None
        }
    }

    pub fn cancel_sign_session(&mut self, session_id: SignSessionId) {
        self.mutate(Mutation::Signing(SigningMutation::CloseSignSession {
            session_id,
            finished: None,
        }))
    }

    pub fn forget_finished_sign_session(
        &mut self,
        session_id: SignSessionId,
    ) -> Option<FinishedSignSession> {
        let session_to_delete = self
            .signing
            .finished_signing_sessions
            .get(&session_id)
            .cloned()?;
        self.mutate(Mutation::Signing(
            SigningMutation::ForgetFinishedSignSession { session_id },
        ));
        Some(session_to_delete)
    }

    pub fn cancel_all_signing_sessions(&mut self) {
        for ssid in self.signing.active_sign_session_order.clone() {
            self.cancel_sign_session(ssid);
        }
    }

    pub fn active_signing_sessions(&self) -> impl Iterator<Item = ActiveSignSession> + '_ {
        self.signing.active_sign_session_order.iter().map(|sid| {
            self.signing
                .active_signing_sessions
                .get(sid)
                .expect("invariant")
                .clone()
        })
    }

    pub fn get_sign_session(&self, session_id: SignSessionId) -> Option<SignSession> {
        if let Some(active) = self.signing.active_signing_sessions.get(&session_id) {
            Some(SignSession::Active(active.clone()))
        } else {
            self.signing
                .finished_signing_sessions
                .get(&session_id)
                .map(|finished| SignSession::Finished(finished.clone()))
        }
    }

    pub fn get_active_sign_session(&self, session_id: SignSessionId) -> Option<&ActiveSignSession> {
        self.signing.get_session(&session_id)
    }

    pub fn active_signing_sessions_by_ssid(&self) -> &BTreeMap<SignSessionId, ActiveSignSession> {
        &self.signing.active_signing_sessions
    }

    pub fn finished_signing_sessions(&self) -> &BTreeMap<SignSessionId, FinishedSignSession> {
        &self.signing.finished_signing_sessions
    }

    /// Handle a signature share arriving for a local signing session. Called
    /// by `recv_signing_message` after confirming the session lives in the
    /// local store. Verifies the share, emits `GotShare` to the user,
    /// persists a `GotSignatureSharesFromDevice` mutation, and emits a
    /// terminal `Signed` message when the session completes.
    fn recv_local_signature_share(
        &mut self,
        from: DeviceId,
        session_id: SignSessionId,
        signature_shares: &[SignatureShare],
        message_kind: &'static str,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        let active_sign_session = self
            .signing
            .get_session(&session_id)
            .expect("caller already verified local session exists");
        let sessions = &active_sign_session.progress;
        let n_signatures = sessions.len();
        let access_structure_ref = active_sign_session.access_structure_ref();
        let access_structure = self
            .get_access_structure(access_structure_ref)
            .expect("session exists so access structure exists");

        let signer_index = access_structure.device_to_share_index.get(&from).ok_or(
            Error::coordinator_invalid_message(
                message_kind,
                "got shares from signer who was not part of the access structure",
            ),
        )?;

        if signature_shares.len() != n_signatures {
            return Err(Error::coordinator_invalid_message(
                message_kind,
                format!(
                    "signer did not provide the right number of signature shares. Got {}, expected {}",
                    signature_shares.len(),
                    sessions.len()
                ),
            ));
        }

        for (session_progress, signature_share) in sessions.iter().zip(signature_shares) {
            let session = &session_progress.sign_session;
            let xonly_frost_key = &session_progress.tweaked_frost_key();
            if !session.parties().contains(signer_index) {
                return Err(Error::coordinator_invalid_message(
                    message_kind,
                    "Signer was not a participant for this session",
                ));
            }

            if session
                .verify_signature_share(
                    xonly_frost_key.verification_share(*signer_index),
                    *signature_share,
                )
                .is_err()
            {
                return Err(Error::coordinator_invalid_message(
                    message_kind,
                    format!(
                        "Invalid signature share under key {}",
                        xonly_frost_key.public_key()
                    ),
                ));
            }
        }

        let participant_shares = ParticipantSignatureShares {
            share_index: *signer_index,
            signature_shares: signature_shares.to_vec(),
        };

        let mut outgoing = vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::Signing(
            CoordinatorToUserSigningMessage::GotShare {
                session_id,
                from,
                shares: participant_shares,
            },
        ))];

        self.mutate(Mutation::Signing(
            SigningMutation::GotSignatureSharesFromDevice {
                session_id,
                device_id: from,
                signature_shares: signature_shares.to_vec(),
            },
        ));

        if let Some(signatures) = self.complete_sign_session(session_id) {
            outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::Signing(
                CoordinatorToUserSigningMessage::Signed {
                    session_id,
                    signatures,
                },
            )));
        }

        Ok(outgoing)
    }

    pub fn recv_signing_message(
        &mut self,
        from: DeviceId,
        message: crate::message::signing::DeviceSigning,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        use crate::message::signing::DeviceSigning;
        let message_kind = message.kind();

        match message {
            DeviceSigning::NonceResponse { segments } => {
                let mut outgoing = vec![];
                for new_segment in segments {
                    self.signing
                        .nonce_cache
                        .check_can_extend(from, &new_segment)
                        .map_err(|e| {
                            Error::coordinator_invalid_message(
                                message_kind,
                                format!("couldn't extend nonces: {e}"),
                            )
                        })?;

                    self.mutate(Mutation::Signing(SigningMutation::NewNonces {
                        device_id: from,
                        nonce_segment: new_segment,
                    }));
                }

                outgoing.push(CoordinatorSend::ToUser(
                    CoordinatorToUserMessage::ReplenishedNonces { device_id: from },
                ));

                Ok(outgoing)
            }
            DeviceSigning::SignatureShare {
                session_id,
                ref signature_shares,
                ref replenish_nonces,
            } => {
                let outgoing = if self.signing.get_session(&session_id).is_some() {
                    self.recv_local_signature_share(
                        from,
                        session_id,
                        signature_shares,
                        message_kind,
                    )?
                } else {
                    self.recv_remote_signature_share(
                        from,
                        session_id,
                        signature_shares,
                        message_kind,
                    )?
                };

                if let Some(replenish_nonces) = replenish_nonces {
                    self.mutate(Mutation::Signing(SigningMutation::NewNonces {
                        device_id: from,
                        nonce_segment: replenish_nonces.clone(),
                    }));
                }

                Ok(outgoing)
            }
        }
    }

    // ========================================================================
    // Nonce reservations
    // ========================================================================

    /// Get all signature shares from an active session, bundled with their share indices.
    pub fn get_signature_shares(
        &self,
        session_id: SignSessionId,
    ) -> Option<Vec<ParticipantSignatureShares>> {
        let session = self.signing.get_session(&session_id)?;
        let access_structure_ref = session.access_structure_ref();
        let access_structure = self.get_access_structure(access_structure_ref)?;

        let mut result = Vec::new();
        for device_id in session.received_from() {
            let share_index = *access_structure.device_to_share_index.get(&device_id)?;
            let signature_shares: Vec<_> = session
                .progress
                .iter()
                .map(|p| *p.signature_shares.get(&device_id).expect("received_from"))
                .collect();
            result.push(ParticipantSignatureShares {
                share_index,
                signature_shares,
            });
        }
        Some(result)
    }

    /// Get signature shares for a specific device from an active session.
    pub fn get_device_signature_shares(
        &self,
        session_id: SignSessionId,
        device_id: DeviceId,
    ) -> Option<ParticipantSignatureShares> {
        let session = self.signing.get_session(&session_id)?;
        if !session.received_from().any(|d| d == device_id) {
            return None;
        }
        let access_structure_ref = session.access_structure_ref();
        let access_structure = self.get_access_structure(access_structure_ref)?;
        let share_index = *access_structure.device_to_share_index.get(&device_id)?;
        let signature_shares: Vec<_> = session
            .progress
            .iter()
            .map(|p| *p.signature_shares.get(&device_id).expect("received_from"))
            .collect();
        Some(ParticipantSignatureShares {
            share_index,
            signature_shares,
        })
    }
}

// ============================================================================
// RequestDeviceSign
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct RequestDeviceSign {
    pub request_sign: RequestSign,
    pub device_id: DeviceId,
}

impl From<RequestDeviceSign> for CoordinatorSend {
    fn from(value: RequestDeviceSign) -> Self {
        CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Signing(
                crate::message::signing::CoordinatorSigning::RequestSign(Box::new(
                    value.request_sign,
                )),
            ),
            destinations: [value.device_id].into(),
        }
    }
}

impl IntoIterator for RequestDeviceSign {
    type Item = CoordinatorSend;

    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(self.into())
    }
}

// ============================================================================
// NonceReplenishRequest
// ============================================================================

pub struct NonceReplenishRequest {
    pub replenish_requests: BTreeMap<DeviceId, Vec<CoordNonceStreamState>>,
}

impl NonceReplenishRequest {
    pub fn some_nonces_requested(&self) -> bool {
        self.replenish_requests
            .values()
            .any(|streams| streams.iter().any(|stream| stream.remaining == 0))
    }

    /// Convert to an iterator of (DeviceId, OpenNonceStreams)
    pub fn into_open_nonce_streams(
        self,
    ) -> impl Iterator<Item = (DeviceId, crate::message::signing::OpenNonceStreams)> {
        self.replenish_requests
            .into_iter()
            .map(|(device_id, streams)| {
                (
                    device_id,
                    crate::message::signing::OpenNonceStreams { streams },
                )
            })
    }
}

impl From<crate::message::signing::OpenNonceStreams> for CoordinatorToDeviceMessage {
    fn from(open: crate::message::signing::OpenNonceStreams) -> Self {
        CoordinatorToDeviceMessage::Signing(
            crate::message::signing::CoordinatorSigning::OpenNonceStreams(open),
        )
    }
}

impl IntoIterator for NonceReplenishRequest {
    type Item = CoordinatorSend;
    type IntoIter = std::vec::IntoIter<CoordinatorSend>;
    fn into_iter(self) -> Self::IntoIter {
        self.replenish_requests
            .into_iter()
            .map(|(device_id, streams)| CoordinatorSend::ToDevice {
                message: crate::message::signing::OpenNonceStreams { streams }.into(),
                destinations: [device_id].into(),
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}
