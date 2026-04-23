use super::signing::*;
use super::*;
use crate::message::EncodedSignature;
use crate::nonce_stream::CoordNonceStreamState;
use crate::sign_task::SignTaskError;
use crate::DeviceId;
use core::fmt;
use schnorr_fun::frost::{self, SignatureShare};
use schnorr_fun::Schnorr;
use sha2::Sha256;

// ============================================================================
// State
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub(super) remote_sign_sessions: BTreeMap<(RemoteSignSessionId, DeviceId), RemoteSignSession>,
}

impl State {
    /// Tmp data used to live in a separate in-memory map. Now everything is
    /// persisted on `RemoteSignSession`, so there's nothing to clear.
    pub fn clear_tmp_data(&mut self) {}

    pub fn apply_mutation(
        &mut self,
        mutation: RemoteSigningMutation,
        nonce_cache: &mut crate::coord_nonces::NonceCache,
    ) -> Option<RemoteSigningMutation> {
        match mutation {
            RemoteSigningMutation::NewRemoteSignSession {
                id,
                device_id,
                ref session,
            } => {
                self.remote_sign_sessions
                    .insert((id, device_id), session.clone());
            }
            RemoteSigningMutation::UseRemoteSignSession {
                id,
                device_id,
                ref sign_session_id,
                ref active,
            } => {
                let session = self.remote_sign_sessions.get_mut(&(id, device_id))?;
                if !matches!(session.status, RemoteSignStatus::Open) {
                    return None;
                }
                session.status = RemoteSignStatus::Used {
                    sign_session_id: *sign_session_id,
                    active: active.clone(),
                };
            }
            RemoteSigningMutation::CompleteSigning {
                id,
                device_id,
                ref shares,
            } => {
                let session = self.remote_sign_sessions.get_mut(&(id, device_id))?;
                let sign_session_id = match &session.status {
                    RemoteSignStatus::Used {
                        sign_session_id, ..
                    } => *sign_session_id,
                    _ => return None,
                };
                let n_signatures = session.binonces.len() as u32;
                let consume_to = session
                    .nonce_state
                    .index
                    .checked_add(n_signatures)
                    .expect("no overflow");
                nonce_cache.consume(device_id, session.nonce_state.stream_id, consume_to);
                session.status = RemoteSignStatus::Completed {
                    sign_session_id,
                    shares: shares.clone(),
                };
            }
            RemoteSigningMutation::CancelRemoteSignSession { id } => {
                let to_remove: Vec<(RemoteSignSessionId, DeviceId)> = self
                    .remote_sign_sessions
                    .keys()
                    .filter(|(rid, _)| *rid == id)
                    .cloned()
                    .collect();
                for key in to_remove {
                    if let Some(session) = self.remote_sign_sessions.remove(&key) {
                        if matches!(session.status, RemoteSignStatus::Used { .. }) {
                            let (_, device_id) = key;
                            let n_signatures = session.binonces.len() as u32;
                            let consume_to = session
                                .nonce_state
                                .index
                                .checked_add(n_signatures)
                                .expect("no overflow");
                            nonce_cache.consume(
                                device_id,
                                session.nonce_state.stream_id,
                                consume_to,
                            );
                        }
                    }
                }
            }
        }
        Some(mutation)
    }

    pub fn all_used_nonce_streams(&self) -> BTreeSet<crate::nonce_stream::NonceStreamId> {
        self.remote_sign_sessions
            .values()
            .map(|r| r.nonce_state.stream_id)
            .collect()
    }

    /// Drop every session whose `key_id` matches. For each removed `Used`
    /// session we also consume the nonces (binonces were exposed to the
    /// device and must not be re-used even if the key is gone). `Open`
    /// sessions have no exposed binonces — just drop them. Called from the
    /// coordinator's `DeleteKey` mutation handler so stale rows can't sit
    /// on the map forever (and so `recv_remote_signature_share` can't
    /// look up an access structure that no longer exists).
    pub fn clear_up_key_deletion(
        &mut self,
        key_id: crate::KeyId,
        nonce_cache: &mut crate::coord_nonces::NonceCache,
    ) {
        let victims: Vec<(RemoteSignSessionId, DeviceId)> = self
            .remote_sign_sessions
            .iter()
            .filter(|(_, session)| session.access_structure_ref.key_id == key_id)
            .map(|(key, _)| *key)
            .collect();
        for key in victims {
            if let Some(session) = self.remote_sign_sessions.remove(&key) {
                if matches!(session.status, RemoteSignStatus::Used { .. }) {
                    let (_, device_id) = key;
                    let n_signatures = session.binonces.len() as u32;
                    let consume_to = session
                        .nonce_state
                        .index
                        .checked_add(n_signatures)
                        .expect("no overflow");
                    nonce_cache.consume(device_id, session.nonce_state.stream_id, consume_to);
                }
            }
        }
    }
}

// ============================================================================
// Mutations
// ============================================================================

#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode, frostsnap_macros::Kind)]
pub enum RemoteSigningMutation {
    /// A new reservation with binonces has been created for a single device.
    /// Status starts as `Open`.
    NewRemoteSignSession {
        id: RemoteSignSessionId,
        device_id: DeviceId,
        session: RemoteSignSession,
    },
    /// The device sign request has been emitted. Status `Open` → `Used`, and
    /// the FROST verification context (`ActiveSignSession`) + `SignSessionId`
    /// are stored on the session so that:
    ///   * a subsequent retry of `sign_with_nonce_reservation` can reproduce
    ///     the same `RequestDeviceSign` without rebuilding state;
    ///   * an inbound signature share can be verified even across app restarts.
    UseRemoteSignSession {
        id: RemoteSignSessionId,
        device_id: DeviceId,
        sign_session_id: SignSessionId,
        active: ActiveSignSession,
    },
    /// Device returned its signature share. Status `Used` →
    /// `Completed { sign_session_id, shares }`, and the reserved nonces are
    /// advanced in the nonce cache (they've been exposed to the device and
    /// must not be reused).
    CompleteSigning {
        id: RemoteSignSessionId,
        device_id: DeviceId,
        shares: ParticipantSignatureShares,
    },
    /// Remove every entry matching `id`. For each removed entry whose status
    /// was `Used`, also consumes its nonces. `Open` sessions can be freely
    /// dropped (nonces never exposed); `Completed` sessions already had
    /// nonces consumed on completion.
    CancelRemoteSignSession { id: RemoteSignSessionId },
}

/// Identifier for a remote signing session, derived by hashing the reserved
/// binonces.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteSignSessionId(pub [u8; 32]);

crate::impl_display_debug_serialize! {
    fn to_bytes(value: &RemoteSignSessionId) -> [u8;32] {
        value.0
    }
}

crate::impl_fromstr_deserialize! {
    name => "remote sign session id",
    fn from_bytes(bytes: [u8;32]) -> RemoteSignSessionId {
        RemoteSignSessionId(bytes)
    }
}

impl RemoteSignSessionId {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn from_binonces(binonces: &[schnorr_fun::binonce::Nonce]) -> Self {
        use sha2::{Digest, Sha256};
        let bytes = bincode::encode_to_vec(binonces, bincode::config::standard())
            .expect("binonce encoding can't fail");
        Self(Sha256::new().chain_update(bytes).finalize().into())
    }
}

/// A remote signing session for a single local device. Persisted; carries
/// enough state across all three lifecycle phases to rebuild a
/// `RequestDeviceSign` (retry) or verify an incoming signature share even
/// after a restart.
///
/// The fields `access_structure_ref` and `sign_task` are committed at
/// `offer_to_sign` time and are immutable for the life of the session. Any
/// subsequent `offer_to_sign` for the same `(RemoteSignSessionId, DeviceId)`
/// must re-supply matching values — otherwise the coordinator rejects the
/// call with `StartSignError::ConflictingRemoteSignSession`. That lets later
/// calls (`sign_with_nonce_reservation`, `recv_remote_signature_share`,
/// `clear_up_key_deletion`) act on the session without the caller
/// re-specifying context that's already known at offer time.
#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct RemoteSignSession {
    pub access_structure_ref: AccessStructureRef,
    pub sign_task: crate::WireSignTask,
    pub binonces: Vec<schnorr_fun::binonce::Nonce>,
    pub nonce_state: CoordNonceStreamState,
    pub status: RemoteSignStatus,
}

#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub enum RemoteSignStatus {
    /// Reservation created. Binonces reserved in the nonce cache but never
    /// handed to the device.
    Open,
    /// Sign request emitted to the device. Carries the FROST verification
    /// context and session id.
    Used {
        sign_session_id: SignSessionId,
        active: ActiveSignSession,
    },
    /// Signature share received and cached; nonces have been consumed.
    Completed {
        sign_session_id: SignSessionId,
        shares: ParticipantSignatureShares,
    },
}

/// Result of `offer_to_sign`.
#[derive(Debug, Clone)]
pub struct SignOffer {
    pub id: RemoteSignSessionId,
    pub device_id: DeviceId,
    pub participant_binonces: ParticipantBinonces,
}

impl FrostCoordinator {
    /// Reserve binonces for a remote signing session. The `(access_structure_ref, sign_task)`
    /// pair is committed at this point — the resulting row can only be used to sign that
    /// exact task under that exact access structure. If a session already exists for
    /// `(id, device_id)`:
    ///   * matching `access_structure_ref` + `sign_task` → returns idempotently (same binonces).
    ///   * any mismatch → `StartSignError::ConflictingRemoteSignSession`.
    ///
    /// The number of binonces to reserve is derived from the sign task — one
    /// per sig-hash item. `sign_task` is also checked against the key's
    /// purpose and master appkey up front so invalid tasks fail here rather
    /// than at sign time.
    pub fn offer_to_sign(
        &mut self,
        id: RemoteSignSessionId,
        access_structure_ref: AccessStructureRef,
        sign_task: crate::WireSignTask,
        device_id: DeviceId,
    ) -> Result<SignOffer, StartSignError> {
        let access_structure = self
            .get_access_structure(access_structure_ref)
            .ok_or(StartSignError::NoSuchAccessStructure)?;
        let share_index = *access_structure
            .device_to_share_indicies()
            .get(&device_id)
            .ok_or(StartSignError::DeviceNotPartOfKey { device_id })?;

        let key_data =
            self.keys
                .get(&access_structure_ref.key_id)
                .ok_or(StartSignError::UnknownKey {
                    key_id: access_structure_ref.key_id,
                })?;
        let checked = sign_task
            .clone()
            .check(key_data.complete_key.master_appkey, key_data.purpose)
            .map_err(StartSignError::SignTask)?;
        let n_signatures = checked.sign_items().len();

        if let Some(existing) = self
            .remote_signing
            .remote_sign_sessions
            .get(&(id, device_id))
        {
            if existing.access_structure_ref != access_structure_ref
                || existing.sign_task != sign_task
            {
                return Err(StartSignError::ConflictingRemoteSignSession);
            }
            return Ok(SignOffer {
                id,
                device_id,
                participant_binonces: ParticipantBinonces {
                    share_index,
                    binonces: existing.binonces.clone(),
                },
            });
        }

        let used_streams: BTreeSet<_> = self
            .signing
            .all_used_nonce_streams()
            .union(&self.remote_signing.all_used_nonce_streams())
            .copied()
            .collect();
        let nonces_map = self
            .signing
            .nonce_cache
            .new_signing_session(
                &[device_id].into_iter().collect(),
                n_signatures,
                &used_streams,
            )
            .map_err(StartSignError::NotEnoughNoncesForDevice)?;

        let nonce_sub_segment = nonces_map.into_values().next().expect("we asked for one");
        let binonces: Vec<schnorr_fun::binonce::Nonce> =
            nonce_sub_segment.segment.nonces.iter().copied().collect();

        let session = RemoteSignSession {
            access_structure_ref,
            sign_task,
            binonces: binonces.clone(),
            nonce_state: nonce_sub_segment.coord_nonce_state(),
            status: RemoteSignStatus::Open,
        };

        self.mutate(Mutation::RemoteSigning(
            RemoteSigningMutation::NewRemoteSignSession {
                id,
                device_id,
                session,
            },
        ));

        Ok(SignOffer {
            id,
            device_id,
            participant_binonces: ParticipantBinonces {
                share_index,
                binonces,
            },
        })
    }

    pub fn cancel_remote_sign_session(&mut self, id: RemoteSignSessionId) {
        self.mutate(Mutation::RemoteSigning(
            RemoteSigningMutation::CancelRemoteSignSession { id },
        ));
    }

    /// Does this `(id, device_id)` reservation exist and is it ready to
    /// sign the given `all_binonces`? Returns false if no reservation
    /// exists, the session has already completed, or a retry's binonces
    /// produce a different `SignSessionId` than the stored `Used` state.
    pub fn can_sign_with_nonce_reservation(
        &self,
        all_binonces: &[ParticipantBinonces],
        id: RemoteSignSessionId,
        device_id: DeviceId,
    ) -> bool {
        let Some(session) = self
            .remote_signing
            .remote_sign_sessions
            .get(&(id, device_id))
        else {
            return false;
        };

        let Some(key_data) = self.keys.get(&session.access_structure_ref.key_id) else {
            return false;
        };
        let Some(access_structure) = key_data
            .complete_key
            .access_structures
            .get(&session.access_structure_ref.access_structure_id)
        else {
            return false;
        };
        if all_binonces.len() < access_structure.threshold() as usize {
            return false;
        }

        match &session.status {
            RemoteSignStatus::Open => true,
            RemoteSignStatus::Used {
                sign_session_id, ..
            } => {
                let group_request = GroupSignReq::from_binonces(
                    session.sign_task.clone(),
                    session.access_structure_ref.access_structure_id,
                    all_binonces,
                );
                group_request.session_id() == *sign_session_id
            }
            RemoteSignStatus::Completed { .. } => false,
        }
    }

    /// Transition an `Open` row to `Used` (or reuse an existing `Used`), and
    /// produce the `RequestDeviceSign` to hand the device. The access
    /// structure and sign task are looked up from the stored session — the
    /// caller doesn't re-supply them. `all_binonces` is the session-wide
    /// participant binonce set (only known after RoundConfirmed).
    pub fn sign_with_nonce_reservation(
        &mut self,
        id: RemoteSignSessionId,
        device_id: DeviceId,
        all_binonces: &[ParticipantBinonces],
        encryption_key: SymmetricKey,
    ) -> Result<RequestDeviceSign, StartSignError> {
        let remote_session = self
            .remote_signing
            .remote_sign_sessions
            .get(&(id, device_id))
            .ok_or(StartSignError::NoSuchRemoteSignSession)?
            .clone();

        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = remote_session.access_structure_ref;

        let key_data = self
            .keys
            .get(&key_id)
            .ok_or(StartSignError::UnknownKey { key_id })?
            .clone();

        let access_structure = key_data
            .complete_key
            .access_structures
            .get(&access_structure_id)
            .ok_or(StartSignError::NoSuchAccessStructure)?;

        let signing_key = super::KeyContext {
            app_shared_key: access_structure.app_shared_key(),
            purpose: key_data.purpose,
        };

        let threshold = access_structure.threshold();
        if all_binonces.len() < threshold as usize {
            return Err(StartSignError::NotEnoughDevicesSelected {
                selected: all_binonces.len(),
                threshold,
            });
        }

        let group_request = GroupSignReq::from_binonces(
            remote_session.sign_task.clone(),
            access_structure_id,
            all_binonces,
        );
        let session_id = group_request.session_id();

        let nonces_for_device = match &remote_session.status {
            RemoteSignStatus::Open => {
                let sessions =
                    build_sign_sessions(&remote_session.sign_task, &signing_key, all_binonces)
                        .map_err(StartSignError::SignTask)?;
                let local_nonces = BTreeMap::from([(device_id, remote_session.nonce_state)]);
                let active = ActiveSignSession {
                    progress: sessions,
                    init: StartSign {
                        local_nonces,
                        group_request: group_request.clone(),
                    },
                    key_id,
                    sent_req_to_device: [device_id].into_iter().collect(),
                };

                self.mutate(Mutation::RemoteSigning(
                    RemoteSigningMutation::UseRemoteSignSession {
                        id,
                        device_id,
                        sign_session_id: session_id,
                        active,
                    },
                ));

                remote_session.nonce_state
            }
            RemoteSignStatus::Used {
                sign_session_id: existing,
                active,
            } => {
                // Idempotent retry: must be the same sign session.
                if *existing != session_id {
                    return Err(StartSignError::NoSuchRemoteSignSession);
                }
                *active
                    .init
                    .local_nonces
                    .get(&device_id)
                    .ok_or(StartSignError::NoSuchRemoteSignSession)?
            }
            RemoteSignStatus::Completed { .. } => {
                return Err(StartSignError::NoSuchRemoteSignSession);
            }
        };

        let (rootkey, coord_share_decryption_contrib) = key_data
            .complete_key
            .coord_share_decryption_contrib(access_structure_id, device_id, encryption_key)
            .expect("must be able to decrypt rootkey");

        Ok(RequestDeviceSign {
            device_id,
            request_sign: RequestSign {
                group_sign_req: group_request,
                device_sign_req: DeviceSignReq {
                    nonces: nonces_for_device,
                    rootkey,
                    coord_share_decryption_contrib,
                },
            },
        })
    }

    /// Handle a signature share arriving for a remote sign session. Called by
    /// `signing::recv_signing_message` when the session isn't in the local
    /// store. Finds the matching `Used` session by `SignSessionId`, verifies
    /// the share against its stored FROST context, emits a `CompleteSigning`
    /// mutation and a `GotShare` to-user message.
    pub fn recv_remote_signature_share(
        &mut self,
        from: DeviceId,
        session_id: SignSessionId,
        signature_shares: &[SignatureShare],
        message_kind: &'static str,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        let ((remote_sign_session_id, remote_device_id), active) = self
            .remote_signing
            .remote_sign_sessions
            .iter()
            .find_map(|(key, session)| match &session.status {
                RemoteSignStatus::Used {
                    sign_session_id: sid,
                    active,
                } if *sid == session_id => Some((*key, active.clone())),
                _ => None,
            })
            .ok_or_else(|| {
                Error::coordinator_invalid_message(message_kind, "no such signing session")
            })?;

        // We only hand out sign requests for the local device that reserved
        // nonces under this session. A share from any other device is bogus.
        if from != remote_device_id {
            return Err(Error::coordinator_invalid_message(
                message_kind,
                "share did not come from the local signing device",
            ));
        }

        let sessions = &active.progress;
        let n_signatures = sessions.len();
        let access_structure_ref = active.access_structure_ref();
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

        self.mutate(Mutation::RemoteSigning(
            RemoteSigningMutation::CompleteSigning {
                id: remote_sign_session_id,
                device_id: from,
                shares: participant_shares.clone(),
            },
        ));

        Ok(vec![CoordinatorSend::ToUser(
            CoordinatorToUserMessage::Signing(CoordinatorToUserSigningMessage::GotShare {
                session_id,
                from,
                shares: participant_shares,
            }),
        )])
    }

    pub fn get_remote_sign_session(
        &self,
        id: RemoteSignSessionId,
        device_id: DeviceId,
    ) -> Option<&RemoteSignSession> {
        self.remote_signing
            .remote_sign_sessions
            .get(&(id, device_id))
    }

    /// All reservations that share a `RemoteSignSessionId` (across different
    /// devices). All entries under the same id must carry identical
    /// `access_structure_ref` and `sign_task` — `offer_to_sign` enforces
    /// that invariant on every insert.
    pub fn remote_sign_sessions_by_id(
        &self,
        id: RemoteSignSessionId,
    ) -> impl Iterator<Item = (DeviceId, &RemoteSignSession)> + '_ {
        self.remote_signing
            .remote_sign_sessions
            .iter()
            .filter(move |((rid, _), _)| *rid == id)
            .map(|((_, device_id), session)| (*device_id, session))
    }

    /// Returns every completed signature share cached under this
    /// `RemoteSignSessionId`, keyed by the device that produced it. Empty if
    /// no device has completed yet (or the session never existed).
    pub fn get_completed_signature_shares(
        &self,
        id: RemoteSignSessionId,
    ) -> BTreeMap<DeviceId, ParticipantSignatureShares> {
        self.remote_signing
            .remote_sign_sessions
            .iter()
            .filter(|((rid, _), _)| *rid == id)
            .filter_map(|((_, device_id), session)| match &session.status {
                RemoteSignStatus::Completed { shares, .. } => Some((*device_id, shares.clone())),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum CombineSignatureError {
    SignTask(SignTaskError),
    NotEnoughShares { got: usize, threshold: usize },
    WrongNumberOfShares { got: usize, expected: usize },
    InvalidShare,
}

impl fmt::Display for CombineSignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CombineSignatureError::SignTask(e) => write!(f, "{e}"),
            CombineSignatureError::NotEnoughShares { got, threshold } => {
                write!(f, "not enough shares: got {got}, need {threshold}")
            }
            CombineSignatureError::WrongNumberOfShares { got, expected } => {
                write!(f, "wrong number of shares: got {got}, expected {expected}")
            }
            CombineSignatureError::InvalidShare => write!(f, "invalid signature share"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CombineSignatureError {}

fn build_sign_sessions(
    sign_task: &crate::WireSignTask,
    signing_key: &super::KeyContext,
    all_binonces: &[ParticipantBinonces],
) -> Result<Vec<SignSessionProgress>, crate::sign_task::SignTaskError> {
    let checked_sign_task = sign_task
        .clone()
        .check(signing_key.master_appkey(), signing_key.purpose)?;
    let sign_items = checked_sign_task.sign_items();
    let frost = frost::new_without_nonce_generation::<Sha256>();

    Ok(sign_items
        .iter()
        .enumerate()
        .map(|(i, sign_item)| {
            let indexed_nonces = all_binonces
                .iter()
                .map(|p| (p.share_index, p.binonces[i]))
                .collect();

            SignSessionProgress::new_deterministic(
                &frost,
                signing_key.app_shared_key.clone(),
                sign_item.clone(),
                indexed_nonces,
            )
        })
        .collect())
}

pub fn verify_signature_shares(
    sign_task: &crate::WireSignTask,
    signing_key: &super::KeyContext,
    all_binonces: &[ParticipantBinonces],
    shares: &ParticipantSignatureShares,
) -> bool {
    let sessions = match build_sign_sessions(sign_task, signing_key, all_binonces) {
        Ok(s) => s,
        Err(_) => return false,
    };

    if shares.signature_shares.len() != sessions.len() {
        return false;
    }

    for (session, signature_share) in sessions.iter().zip(&shares.signature_shares) {
        let xonly_frost_key = session.tweaked_frost_key();
        if session
            .sign_session
            .verify_signature_share(
                xonly_frost_key.verification_share(shares.share_index),
                *signature_share,
            )
            .is_err()
        {
            return false;
        }
    }

    true
}

pub fn combine_signatures(
    sign_task: crate::WireSignTask,
    signing_key: &super::KeyContext,
    all_binonces: &[ParticipantBinonces],
    all_shares: &[&ParticipantSignatureShares],
) -> Result<Vec<EncodedSignature>, CombineSignatureError> {
    let sessions = build_sign_sessions(&sign_task, signing_key, all_binonces)
        .map_err(CombineSignatureError::SignTask)?;
    let n_signatures = sessions.len();

    let threshold = signing_key.app_shared_key.key.threshold();
    if all_shares.len() < threshold {
        return Err(CombineSignatureError::NotEnoughShares {
            got: all_shares.len(),
            threshold,
        });
    }

    for shares in all_shares {
        if shares.signature_shares.len() != n_signatures {
            return Err(CombineSignatureError::WrongNumberOfShares {
                got: shares.signature_shares.len(),
                expected: n_signatures,
            });
        }

        for (session, signature_share) in sessions.iter().zip(&shares.signature_shares) {
            let xonly_frost_key = session.tweaked_frost_key();
            if session
                .sign_session
                .verify_signature_share(
                    xonly_frost_key.verification_share(shares.share_index),
                    *signature_share,
                )
                .is_err()
            {
                return Err(CombineSignatureError::InvalidShare);
            }
        }
    }

    let schnorr = Schnorr::<Sha256, _>::verify_only();
    let signatures = sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let shares = all_shares.iter().map(|p| p.signature_shares[i]);
            let sig = session.sign_session.combine_signature_shares(shares);
            assert!(
                session.verify_final_signature(&schnorr, &sig),
                "verified shares so combined signature must be valid"
            );
            EncodedSignature::new(sig)
        })
        .collect();

    Ok(signatures)
}
