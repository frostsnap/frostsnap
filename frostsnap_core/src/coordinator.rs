use crate::{
    coord_nonces::{NonceCache, NotEnoughNonces},
    device::KeyPurpose,
    message::*,
    nonce_stream::{CoordNonceStreamState, NonceStreamId, NonceStreamSegment},
    symmetric_encryption::{Ciphertext, SymmetricKey},
    tweak::Xpub,
    AccessStructureId, AccessStructureKind, AccessStructureRef, ActionError,
    CoordShareDecryptionContrib, DeviceId, Error, Gist, KeyId, KeygenId, Kind, MasterAppkey,
    MessageResult, RestorationId, SessionHash, ShareImage, SignItem, SignSessionId, SignTaskError,
    WireSignTask, NONCE_BATCH_SIZE,
};
use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use frostsnap_macros::Kind;
use schnorr_fun::{
    frost::{
        self, chilldkg::encpedpop, CoordinatorSignSession, Frost, PartyIndex, SharedKey,
        SignatureShare,
    },
    fun::{poly, prelude::*},
    Schnorr, Signature,
};
use sha2::Sha256;
use std::collections::HashMap;
use tracing::{event, Level};

mod coordinator_to_user;
pub mod restoration;
pub use coordinator_to_user::*;

pub const MIN_NONCES_BEFORE_REQUEST: u32 = NONCE_BATCH_SIZE / 2;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrostCoordinator {
    keys: BTreeMap<KeyId, CoordFrostKey>,
    key_order: Vec<KeyId>,
    pending_keygens: HashMap<KeygenId, KeyGenState>,
    nonce_cache: NonceCache,
    mutations: VecDeque<Mutation>,
    active_signing_sessions: BTreeMap<SignSessionId, ActiveSignSession>,
    active_sign_session_order: Vec<SignSessionId>,
    finished_signing_sessions: BTreeMap<SignSessionId, FinishedSignSession>,
    restoration: restoration::State,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CoordFrostKey {
    pub key_id: KeyId,
    pub complete_key: CompleteKey,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CompleteKey {
    pub master_appkey: MasterAppkey,
    pub encrypted_rootkey: Ciphertext<33, Point>,
    pub access_structures: HashMap<AccessStructureId, CoordAccessStructure>,
}

impl CompleteKey {
    pub fn coord_share_decryption_contrib(
        &self,
        access_structure_id: AccessStructureId,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> Option<(Point, CoordShareDecryptionContrib)> {
        let root_shared_key = self.root_shared_key(access_structure_id, encryption_key)?;
        let share_index = *self
            .access_structures
            .get(&access_structure_id)?
            .device_to_share_index
            .get(&device_id)?;
        Some((
            root_shared_key.public_key(),
            CoordShareDecryptionContrib::for_master_share(device_id, share_index, &root_shared_key),
        ))
    }

    pub fn root_shared_key(
        &self,
        access_structure_id: AccessStructureId,
        encryption_key: SymmetricKey,
    ) -> Option<SharedKey> {
        let access_structure = self.access_structures.get(&access_structure_id)?;
        let rootkey = self.encrypted_rootkey.decrypt(encryption_key)?;
        let mut poly = access_structure
            .app_shared_key
            .key
            .point_polynomial()
            .to_vec();
        poly[0] = rootkey.mark_zero();
        debug_assert!(
            MasterAppkey::derive_from_rootkey(rootkey) == access_structure.master_appkey()
        );
        Some(SharedKey::from_poly(poly).non_zero().expect("invariant"))
    }
}

#[cfg(feature = "coordinator")]
#[macro_export]
macro_rules! fail {
    ($($fail:tt)*) => {{
        tracing::event!(
            tracing::Level::ERROR,
            $($fail)*
        );
        debug_assert!(false, $($fail)*);
        return None;
    }};
}

impl CoordFrostKey {
    pub fn get_access_structure(
        &self,
        access_structure_id: AccessStructureId,
    ) -> Option<CoordAccessStructure> {
        self.complete_key
            .access_structures
            .get(&access_structure_id)
            .cloned()
    }

    pub fn access_structures(&self) -> impl Iterator<Item = CoordAccessStructure> + '_ {
        self.complete_key.access_structures.values().cloned()
    }

    pub fn master_access_structure(&self) -> CoordAccessStructure {
        self.access_structures().next().unwrap()
    }
}

impl FrostCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mutate(&mut self, mutation: Mutation) {
        let kind = mutation.kind();
        if let Some(reduced_mutation) = self.apply_mutation(mutation) {
            event!(Level::DEBUG, gist = reduced_mutation.gist(), "mutating");
            self.mutations.push_back(reduced_mutation);
        } else {
            event!(Level::DEBUG, kind = kind, "ignoring mutation");
        }
    }

    pub fn apply_mutation(&mut self, mutation: Mutation) -> Option<Mutation> {
        fn ensure_key<'a>(
            coord: &'a mut FrostCoordinator,
            complete_key: self::CompleteKey,
            key_name: &str,
            purpose: KeyPurpose,
        ) -> &'a mut CoordFrostKey {
            let key_id = complete_key.master_appkey.key_id();
            let exists = coord.keys.contains_key(&key_id);
            let key = coord.keys.entry(key_id).or_insert_with(|| CoordFrostKey {
                key_id,
                complete_key,
                key_name: key_name.to_owned(),
                purpose,
            });
            if !exists {
                coord.key_order.push(key_id);
            }
            key
        }
        use Mutation::*;
        match mutation {
            NewKey {
                ref complete_key,
                ref key_name,
                purpose,
            } => {
                ensure_key(self, complete_key.clone(), key_name, purpose);
            }
            NewAccessStructure {
                ref shared_key,
                kind,
            } => {
                let access_structure_id =
                    AccessStructureId::from_app_poly(shared_key.key().point_polynomial());
                let appkey = MasterAppkey::from_xpub_unchecked(shared_key);
                let key_id = appkey.key_id();

                match self.keys.get_mut(&key_id) {
                    Some(key_data) => {
                        let exists = key_data
                            .complete_key
                            .access_structures
                            .contains_key(&access_structure_id);

                        if exists {
                            return None;
                        }

                        key_data.complete_key.access_structures.insert(
                            access_structure_id,
                            CoordAccessStructure {
                                app_shared_key: shared_key.clone(),
                                device_to_share_index: Default::default(),
                                kind,
                            },
                        );
                    }
                    None => {
                        fail!("access structure added to non-existent key");
                    }
                }
            }
            NewShare {
                access_structure_ref,
                device_id,
                share_index,
            } => match self.keys.get_mut(&access_structure_ref.key_id) {
                Some(key_data) => {
                    let complete_key = &mut key_data.complete_key;

                    match complete_key
                        .access_structures
                        .get_mut(&access_structure_ref.access_structure_id)
                    {
                        Some(access_structure) => {
                            let changed = access_structure
                                .device_to_share_index
                                .insert(device_id, share_index)
                                != Some(share_index);

                            if !changed {
                                return None;
                            }
                        }
                        None => {
                            fail!(
                                "share added to non-existent access structure {:?}",
                                access_structure_ref
                            );
                        }
                    }
                }
                None => {
                    fail!(
                        "share added to non-existent key: {}",
                        access_structure_ref.key_id
                    );
                }
            },
            DeleteKey(key_id) => {
                self.keys.remove(&key_id)?;
                self.key_order.retain(|&entry| entry != key_id);
                self.restoration.clear_up_key_deletion(key_id);
                let sessions_to_delete = self
                    .active_signing_sessions
                    .iter()
                    .filter(|(_, session)| session.key_id == key_id)
                    .map(|(&key_id, _)| key_id)
                    .collect::<Vec<_>>();
                for session_id in sessions_to_delete {
                    self.apply_mutation(Mutation::CloseSignSession {
                        session_id,
                        finished: None,
                    });
                }
            }
            NewNonces {
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
            NewSigningSession(ref signing_session_state) => {
                let ssid = signing_session_state.init.group_request.session_id();
                self.active_signing_sessions
                    .insert(ssid, signing_session_state.clone());
                self.active_sign_session_order.push(ssid);
            }
            GotSignatureSharesFromDevice {
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
            SentSignReq {
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
            CloseSignSession {
                session_id,
                ref finished,
            } => {
                let (index, _) = self
                    .active_sign_session_order
                    .iter()
                    .enumerate()
                    .find(|(_, ssid)| **ssid == session_id)?;
                self.active_sign_session_order.remove(index);
                let session_state = self.active_signing_sessions.remove(&session_id).unwrap();

                for (device_id, nonce_segment) in &session_state.init.nonces {
                    if session_state.sent_req_to_device.contains(device_id) {
                        let state_after_signing = nonce_segment
                            .after_signing(session_state.init.group_request.n_signatures());
                        self.nonce_cache.consume(
                            *device_id,
                            state_after_signing.stream_id,
                            state_after_signing.index,
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
            }
            ForgetFinishedSignSession { session_id } => {
                self.finished_signing_sessions.remove(&session_id);
            }
            Restoration(inner) => {
                return self
                    .restoration
                    .apply_mutation_restoration(inner)
                    .map(Mutation::Restoration);
            }
        }

        Some(mutation)
    }

    pub fn take_staged_mutations(&mut self) -> VecDeque<Mutation> {
        core::mem::take(&mut self.mutations)
    }

    pub fn staged_mutations(&self) -> &VecDeque<Mutation> {
        &self.mutations
    }

    pub fn iter_keys(&self) -> impl Iterator<Item = &CoordFrostKey> + '_ {
        self.key_order
            .iter()
            .map(|key_id| self.keys.get(key_id).expect("invariant"))
    }

    pub fn iter_access_structures(&self) -> impl Iterator<Item = CoordAccessStructure> + '_ {
        self.keys
            .iter()
            .flat_map(|(_, key_data)| key_data.access_structures())
    }

    pub fn get_frost_key(&self, key_id: KeyId) -> Option<&CoordFrostKey> {
        self.keys.get(&key_id)
    }

    pub fn recv_device_message(
        &mut self,
        from: DeviceId,
        message: DeviceToCoordinatorMessage,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        let message_kind = message.kind();
        match message {
            DeviceToCoordinatorMessage::NonceResponse { segments } => {
                for new_segment in segments {
                    self.nonce_cache
                        .check_can_extend(from, &new_segment)
                        .map_err(|e| {
                            Error::coordinator_invalid_message(
                                message_kind,
                                format!("couldn't extend nonces: {e}"),
                            )
                        })?;

                    self.mutate(Mutation::NewNonces {
                        device_id: from,
                        nonce_segment: new_segment,
                    });
                }

                Ok(vec![])
            }
            DeviceToCoordinatorMessage::KeyGenResponse(response) => {
                let keygen_id = response.keygen_id;
                match self.pending_keygens.get_mut(&keygen_id) {
                    Some(KeyGenState::WaitingForResponses {
                        input_aggregator,
                        device_to_share_index,
                        pending_key_name,
                        purpose,
                        ..
                    }) => {
                        let device_to_share_index = device_to_share_index.clone();
                        let share_index = device_to_share_index.get(&from).ok_or(
                            Error::coordinator_invalid_message(
                                message_kind,
                                "got share from device that was not part of keygen",
                            ),
                        )?;

                        input_aggregator
                            .add_input(
                                &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                                // we use the share index as the input generator index. The input
                                // generator at index 0 is the coordinator itself.
                                (*share_index).into(),
                                *response.input,
                            )
                            .map_err(|e| Error::coordinator_invalid_message(message_kind, e))?;

                        let mut outgoing =
                            vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                                keygen_id,
                                inner: CoordinatorToUserKeyGenMessage::ReceivedShares { from },
                            })];

                        if input_aggregator.is_finished() {
                            let agg_input = input_aggregator.clone().finish().unwrap();
                            let session_hash = SessionHash::from_agg_input(&agg_input);
                            outgoing.push(CoordinatorSend::ToDevice {
                                destinations: device_to_share_index.keys().cloned().collect(),
                                message: CoordinatorToDeviceMessage::FinishKeyGen {
                                    keygen_id,
                                    agg_input: agg_input.clone(),
                                },
                            });

                            outgoing.push(CoordinatorSend::ToUser(
                                CoordinatorToUserMessage::KeyGen {
                                    keygen_id,
                                    inner: CoordinatorToUserKeyGenMessage::CheckKeyGen {
                                        session_hash,
                                    },
                                },
                            ));

                            let new_state = KeyGenState::WaitingForAcks {
                                agg_input: agg_input.clone(),
                                device_to_share_index: device_to_share_index
                                    .into_iter()
                                    .map(|(device, share_index)| {
                                        (device, PartyIndex::from(share_index))
                                    })
                                    .collect(),
                                acks: Default::default(),
                                pending_key_name: pending_key_name.clone(),
                                purpose: *purpose,
                            };

                            self.pending_keygens.insert(keygen_id, new_state);
                        }
                        Ok(outgoing)
                    }
                    _ => Err(Error::coordinator_invalid_message(
                        message_kind,
                        "keygen wasn't in WaitingForResponses state",
                    )),
                }
            }
            DeviceToCoordinatorMessage::KeyGenAck(self::KeyGenAck {
                keygen_id,
                ack_session_hash,
            }) => {
                let mut outgoing = vec![];
                match self.pending_keygens.get_mut(&keygen_id) {
                    Some(KeyGenState::WaitingForAcks {
                        agg_input,
                        device_to_share_index,
                        acks,
                        ..
                    }) => {
                        let session_hash = SessionHash::from_agg_input(agg_input);

                        if ack_session_hash != session_hash {
                            return Err(Error::coordinator_invalid_message(
                                message_kind,
                                "Device acked wrong keygen session hash",
                            ));
                        }

                        if !device_to_share_index.contains_key(&from) {
                            return Err(Error::coordinator_invalid_message(
                                message_kind,
                                "Received ack from device not a member of keygen",
                            ));
                        }

                        if acks.insert(from) {
                            let all_acks_received = acks.len() == device_to_share_index.len();

                            outgoing.push(CoordinatorSend::ToUser(
                                CoordinatorToUserMessage::KeyGen {
                                    inner: CoordinatorToUserKeyGenMessage::KeyGenAck {
                                        from,
                                        all_acks_received,
                                    },
                                    keygen_id,
                                },
                            ));
                        }

                        Ok(outgoing)
                    }
                    _ => Err(Error::coordinator_invalid_message(
                        message_kind,
                        "received ACK for keygen but this keygen wasn't in WaitingForAcks state",
                    )),
                }
            }

            DeviceToCoordinatorMessage::SignatureShare {
                session_id,
                ref signature_shares,
                ref replenish_nonces,
            } => {
                let active_sign_session = self
                    .active_signing_sessions
                    .get(&session_id)
                    .expect("inavariant");
                let sessions = &active_sign_session.progress;
                let n_signatures = sessions.len();
                let access_structure_ref = active_sign_session.access_structure_ref();
                let access_structure = self
                    .get_access_structure(access_structure_ref)
                    .expect("session exists access structure exists");
                let mut outgoing = vec![];
                let signer_index = access_structure.device_to_share_index.get(&from).ok_or(
                    Error::coordinator_invalid_message(
                        message_kind,
                        "got shares from signer who was not part of the access structure",
                    ),
                )?;

                if signature_shares.len() != n_signatures {
                    return Err(Error::coordinator_invalid_message(message_kind, format!("signer did not provide the right number of signature shares. Got {}, expected {}", signature_shares.len(), sessions.len())));
                }

                for (session_progress, signature_share) in sessions.iter().zip(signature_shares) {
                    let session = &session_progress.sign_session;
                    let xonly_frost_key = &session_progress.tweaked_frost_key();
                    if !session.parties().contains(signer_index) {
                        return Err(Error::coordinator_invalid_message(
                            message_kind,
                            "Signer was not a particpant for this session",
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

                outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::Signing(
                    CoordinatorToUserSigningMessage::GotShare { session_id, from },
                )));

                self.mutate(Mutation::GotSignatureSharesFromDevice {
                    session_id,
                    device_id: from,
                    signature_shares: signature_shares.clone(),
                });

                if let Some(replenish_nonces) = replenish_nonces {
                    self.mutate(Mutation::NewNonces {
                        device_id: from,
                        nonce_segment: replenish_nonces.clone(),
                    });
                }

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
            DeviceToCoordinatorMessage::Restoration(message) => {
                self.recv_restoration_message(from, message)
            }
        }
    }

    pub fn do_keygen(
        &mut self,
        do_keygen: DoKeyGen,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<SendDokeygen, ActionError> {
        let DoKeyGen {
            device_to_share_index,
            threshold,
            key_name,
            purpose,
            keygen_id,
        } = &do_keygen;

        if self.pending_keygens.contains_key(&do_keygen.keygen_id) {
            return Err(ActionError::StateInconsistent(
                "keygen with that id already in progress".into(),
            ));
        }

        let n_devices = device_to_share_index.len();

        if n_devices < *threshold as usize {
            panic!(
                "caller needs to ensure that threshold < devices.len(). Tried {}-of-{}",
                threshold, n_devices
            );
        }
        let share_receivers_enckeys = device_to_share_index
            .iter()
            .map(|(device, share_index)| (PartyIndex::from(*share_index), device.pubkey()))
            .collect::<BTreeMap<_, _>>();
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
        let mut input_aggregator = encpedpop::Coordinator::new(
            (*threshold).into(),
            (n_devices + 1) as u32,
            &share_receivers_enckeys,
        );
        // We don't need to keep the _coordinator_inputter state since we are the one forming agg_input
        let (_coordinator_inputter, input) = encpedpop::Contributor::gen_keygen_input(
            &schnorr,
            (*threshold).into(),
            &share_receivers_enckeys,
            0,
            rng,
        );
        input_aggregator
            .add_input(&schnorr, 0, input)
            .expect("we just generated the input");

        self.pending_keygens.insert(
            *keygen_id,
            KeyGenState::WaitingForResponses {
                keygen_id: *keygen_id,
                input_aggregator,
                device_to_share_index: device_to_share_index.clone(),
                pending_key_name: key_name.to_string(),
                purpose: *purpose,
            },
        );

        Ok(SendDokeygen(do_keygen.clone()))
    }

    /// This is called when the user has checked every device agrees and finally confirms this with
    /// the coordinator.
    pub fn final_keygen_ack(
        &mut self,
        keygen_id: KeygenId,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<AccessStructureRef, ActionError> {
        match self.pending_keygens.get(&keygen_id) {
            Some(KeyGenState::WaitingForAcks {
                device_to_share_index,
                agg_input,
                acks,
                pending_key_name,
                purpose,
            }) => {
                let all_acks = acks.len() == device_to_share_index.len();
                if all_acks {
                    let root_shared_key = agg_input
                        .shared_key()
                        .non_zero()
                        .expect("this should have already been checked");
                    let access_structure_ref = self.mutate_new_key(
                        pending_key_name.clone(),
                        root_shared_key,
                        device_to_share_index.clone(),
                        encryption_key,
                        *purpose,
                        rng,
                    );
                    self.pending_keygens.remove(&keygen_id);
                    Ok(access_structure_ref)
                } else {
                    Err(ActionError::StateInconsistent(
                        "all device acks have not been received yet".into(),
                    ))
                }
            }
            _ => Err(ActionError::StateInconsistent("no such keygen".into())),
        }
    }

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
            .nonce_cache
            .new_signing_session(
                signing_devices,
                n_signatures,
                &self.all_used_nonce_streams(),
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
            nonces: device_requests,
            group_request,
        };

        let local_session = ActiveSignSession {
            progress: sessions,
            init: start_sign.clone(),
            key_id,
            sent_req_to_device: Default::default(),
        };

        self.mutate(Mutation::NewSigningSession(local_session));

        Ok(session_id)
    }

    pub fn request_device_sign(
        &mut self,
        session_id: SignSessionId,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> RequestDeviceSign {
        let active_sign_session = self
            .active_signing_sessions
            .get(&session_id)
            .expect("signing session doesn't exist");

        let nonces_for_device = *active_sign_session
            .init
            .nonces
            .get(&device_id)
            .expect("device must be part of the signing session");

        let key_id = active_sign_session.key_id;

        let complete_key = &self.keys.get(&key_id).expect("key exists").complete_key;

        let group_sign_req = active_sign_session.init.group_request.clone();
        let (rootkey, coord_share_decryption_contrib) = complete_key
            .coord_share_decryption_contrib(
                group_sign_req.access_structure_id,
                device_id,
                encryption_key,
            )
            .expect("must be able to decrypt rootkey");

        self.mutate(Mutation::SentSignReq {
            device_id,
            session_id,
        });

        let request_sign = RequestSign {
            group_sign_req,
            device_sign_req: DeviceSignReq {
                nonces: nonces_for_device,
                rootkey,
                coord_share_decryption_contrib,
            },
        };

        RequestDeviceSign {
            device_id,
            request_sign,
        }
    }

    pub fn maybe_request_nonce_replenishment(
        &self,
        device_id: DeviceId,
        desired_nonce_streams: usize,
        rng: &mut impl rand_core::RngCore,
    ) -> impl IntoIterator<Item = CoordinatorSend> {
        core::iter::once(CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::OpenNonceStreams {
                streams: self
                    .nonce_cache
                    .generate_nonce_stream_opening_requests(device_id, desired_nonce_streams, rng)
                    .into_iter()
                    .collect(),
            },
            destinations: [device_id].into(),
        })
    }

    pub fn verify_address(
        &self,
        key_id: KeyId,
        derivation_index: u32,
    ) -> Result<VerifyAddress, ActionError> {
        let frost_key = self
            .get_frost_key(key_id)
            .ok_or(ActionError::StateInconsistent("no such frost key".into()))?;

        let master_appkey = frost_key.complete_key.master_appkey;

        // verify on any device that knows about this key
        let target_devices: BTreeSet<_> = frost_key
            .access_structures()
            .flat_map(|accss| {
                accss
                    .device_to_share_index
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();

        Ok(VerifyAddress {
            master_appkey,
            derivation_index,
            target_devices,
        })
    }

    pub fn nonces_available(&self, device_id: DeviceId) -> BTreeMap<NonceStreamId, u32> {
        self.nonce_cache
            .nonces_available(device_id, &self.all_used_nonce_streams())
    }

    pub fn get_access_structure(
        &self,
        access_structure_ref: AccessStructureRef,
    ) -> Option<CoordAccessStructure> {
        let key = self.keys.get(&access_structure_ref.key_id)?;
        let access_structure =
            key.get_access_structure(access_structure_ref.access_structure_id)?;
        Some(access_structure)
    }

    fn mutate_new_key(
        &mut self,
        name: String,
        root_shared_key: SharedKey,
        device_to_share_index: BTreeMap<DeviceId, PartyIndex>,
        encryption_key: SymmetricKey,
        purpose: KeyPurpose,
        rng: &mut impl rand_core::RngCore,
    ) -> AccessStructureRef {
        let rootkey = root_shared_key.public_key();
        let root_shared_key = Xpub::from_rootkey(root_shared_key);
        let app_shared_key = root_shared_key.rootkey_to_master_appkey();
        let access_structure_id =
            AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());
        let encrypted_rootkey = Ciphertext::encrypt(encryption_key, &rootkey, rng);
        let master_appkey = MasterAppkey::from_xpub_unchecked(&app_shared_key);
        let key_id = master_appkey.key_id();
        let access_structure_ref = AccessStructureRef {
            key_id,
            access_structure_id,
        };

        if self.get_frost_key(key_id).is_none() {
            self.mutate(Mutation::NewKey {
                key_name: name,
                purpose,
                complete_key: CompleteKey {
                    master_appkey,
                    encrypted_rootkey,
                    access_structures: Default::default(),
                },
            });
        }

        self.mutate(Mutation::NewAccessStructure {
            shared_key: app_shared_key,
            kind: AccessStructureKind::Master,
        });

        for (device_id, share_index) in device_to_share_index {
            self.mutate(Mutation::NewShare {
                access_structure_ref,
                device_id,
                share_index,
            });
        }

        access_structure_ref
    }

    pub fn delete_key(&mut self, key_id: KeyId) {
        if self.keys.contains_key(&key_id) {
            self.mutate(Mutation::DeleteKey(key_id));
        }
    }

    pub fn all_used_nonce_streams(&self) -> BTreeSet<NonceStreamId> {
        self.active_signing_sessions
            .values()
            .flat_map(|session| {
                session
                    .init
                    .nonces
                    .values()
                    .map(|device_nonces| device_nonces.stream_id)
            })
            .collect()
    }

    pub fn complete_sign_session(
        &mut self,
        session_id: SignSessionId,
    ) -> Option<Vec<EncodedSignature>> {
        let this = &self;
        let sign_state = this.active_signing_sessions.get(&session_id)?;
        let sessions = &sign_state.progress;

        let all_finished = sessions.iter().all(|session| {
            session.signature_shares.len() == sign_state.init.group_request.parties.len()
        });

        if all_finished {
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

            self.mutate(Mutation::CloseSignSession {
                session_id,
                finished: Some(signatures.clone()),
            });

            Some(signatures)
        } else {
            None
        }
    }

    pub fn cancel_sign_session(&mut self, session_id: SignSessionId) {
        self.mutate(Mutation::CloseSignSession {
            session_id,
            finished: None,
        })
    }

    pub fn forget_finished_sign_session(
        &mut self,
        session_id: SignSessionId,
    ) -> Option<FinishedSignSession> {
        let session_to_delete = self.finished_signing_sessions.get(&session_id).cloned()?;
        self.mutate(Mutation::ForgetFinishedSignSession { session_id });
        Some(session_to_delete)
    }

    pub fn cancel_all_signing_sessions(&mut self) {
        for ssid in self.active_sign_session_order.clone() {
            self.cancel_sign_session(ssid);
        }
    }

    pub fn active_signing_sessions(&self) -> impl Iterator<Item = ActiveSignSession> + '_ {
        self.active_sign_session_order.iter().map(|sid| {
            self.active_signing_sessions
                .get(sid)
                .expect("invariant")
                .clone()
        })
    }

    pub fn active_signing_sessions_by_ssid(&self) -> &BTreeMap<SignSessionId, ActiveSignSession> {
        &self.active_signing_sessions
    }

    pub fn finished_signing_sessions(&self) -> &BTreeMap<SignSessionId, FinishedSignSession> {
        &self.finished_signing_sessions
    }

    pub fn cancel_keygen(&mut self, keygen_id: KeygenId) {
        let _ = self.pending_keygens.remove(&keygen_id);
    }

    pub fn cancel_all_keygens(&mut self) {
        self.pending_keygens.clear()
    }

    pub fn knows_about_share(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        index: PartyIndex,
    ) -> bool {
        let already_got_under_key = self
            .keys
            .get(&access_structure_ref.key_id)
            .and_then(|coord_key| {
                let access_structure_id = access_structure_ref.access_structure_id;
                let existing_index = coord_key
                    .get_access_structure(access_structure_id)?
                    .device_to_share_index
                    .get(&device_id)
                    .copied();

                Some(existing_index == Some(index))
            })
            .unwrap_or(false);

        already_got_under_key
    }

    pub fn root_shared_key(
        &self,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
    ) -> Option<SharedKey> {
        let frost_key = self.get_frost_key(access_structure_ref.key_id)?;

        let root_shared_key = frost_key
            .complete_key
            .root_shared_key(access_structure_ref.access_structure_id, encryption_key)?;
        Some(root_shared_key)
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SignSessionProgress {
    sign_item: SignItem,
    sign_session: CoordinatorSignSession,
    signature_shares: BTreeMap<DeviceId, SignatureShare>,
    app_shared_key: Xpub<SharedKey>,
}

impl SignSessionProgress {
    pub fn new<NG>(
        frost: &Frost<sha2::Sha256, NG>,
        app_shared_key: Xpub<SharedKey>,
        sign_item: SignItem,
        nonces: BTreeMap<frost::PartyIndex, frost::Nonce>,
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
    pub sent_req_to_device: BTreeSet<DeviceId>,
}

impl ActiveSignSession {
    pub fn access_structure_ref(&self) -> AccessStructureRef {
        AccessStructureRef {
            key_id: self.key_id,
            access_structure_id: self.init.group_request.access_structure_id,
        }
    }

    pub fn received_from(&self) -> impl Iterator<Item = DeviceId> + '_ {
        // all sessions make progress at the same time
        self.progress[0].received_from()
    }

    pub fn has_received_from(&self, device_id: DeviceId) -> bool {
        self.progress[0].signature_shares.contains_key(&device_id)
    }

    pub fn session_id(&self) -> SignSessionId {
        self.init.group_request.session_id()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct FinishedSignSession {
    pub init: StartSign,
    pub signatures: Vec<EncodedSignature>,
    pub key_id: KeyId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyGenState {
    WaitingForResponses {
        keygen_id: KeygenId,
        input_aggregator: encpedpop::Coordinator,
        device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
        pending_key_name: String,
        purpose: KeyPurpose,
    },
    WaitingForAcks {
        agg_input: encpedpop::AggKeygenInput,
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        acks: BTreeSet<DeviceId>,
        pending_key_name: String,
        purpose: KeyPurpose,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CoordAccessStructure {
    app_shared_key: Xpub<SharedKey>,
    device_to_share_index: BTreeMap<DeviceId, PartyIndex>,
    kind: crate::AccessStructureKind,
}

impl CoordAccessStructure {
    pub fn threshold(&self) -> u16 {
        self.app_shared_key
            .key
            .threshold()
            .try_into()
            .expect("threshold too large")
    }

    pub fn access_structure_ref(&self) -> AccessStructureRef {
        AccessStructureRef {
            key_id: self.master_appkey().key_id(),
            access_structure_id: self.access_structure_id(),
        }
    }

    pub fn app_shared_key(&self) -> Xpub<SharedKey> {
        self.app_shared_key.clone()
    }

    pub fn master_appkey(&self) -> MasterAppkey {
        MasterAppkey::from_xpub_unchecked(&self.app_shared_key)
    }

    pub fn devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_to_share_index.keys().cloned()
    }

    pub fn contains_device(&self, id: DeviceId) -> bool {
        self.device_to_share_index.contains_key(&id)
    }

    pub fn access_structure_id(&self) -> AccessStructureId {
        AccessStructureId::from_app_poly(self.app_shared_key.key.point_polynomial())
    }

    pub fn device_to_share_indicies(&self) -> BTreeMap<DeviceId, Scalar<Public, NonZero>> {
        self.device_to_share_index.clone()
    }
}

#[derive(Debug, Clone)]
pub enum StartSignError {
    UnknownKey { key_id: KeyId },
    DeviceNotPartOfKey { device_id: DeviceId },
    NotEnoughDevicesSelected { selected: usize, threshold: u16 },
    CantSignInState { in_state: &'static str },
    NotEnoughNoncesForDevice(NotEnoughNonces),
    SignTask(SignTaskError),
    NoSuchAccessStructure,
    CouldntDecryptRootKey,
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
                    "Need more than {} signers for threshold {}",
                    selected, threshold
                )
            }
            StartSignError::CantSignInState { in_state } => {
                write!(f, "Can't sign in state {}", in_state)
            }
            StartSignError::NotEnoughNoncesForDevice(not_enough_nonces) => not_enough_nonces.fmt(f),
            StartSignError::DeviceNotPartOfKey { device_id } => {
                write!(
                    f,
                    "Don't know the share index for device that was part of sign request. ID: {}",
                    device_id
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
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StartSignError {}

/// Mutations to the coordinator state
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, Kind)]
pub enum Mutation {
    NewKey {
        key_name: String,
        purpose: KeyPurpose,
        complete_key: CompleteKey,
    },
    NewAccessStructure {
        shared_key: Xpub<SharedKey>,
        kind: AccessStructureKind,
    },
    NewShare {
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
        share_index: PartyIndex,
    },
    DeleteKey(KeyId),
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
    #[delegate_kind]
    Restoration(restoration::RestorationMutation),
}

impl Mutation {
    pub fn tied_to_key(&self) -> Option<KeyId> {
        Some(match self {
            Mutation::NewKey { complete_key, .. } => complete_key.master_appkey.key_id(),
            Mutation::NewAccessStructure { shared_key, .. } => {
                MasterAppkey::from_xpub_unchecked(shared_key).key_id()
            }
            Mutation::NewShare {
                access_structure_ref,
                ..
            } => access_structure_ref.key_id,
            Mutation::DeleteKey(key_id) => *key_id,
            Mutation::Restoration(inner) => inner.tied_to_key()?,
            _ => return None,
        })
    }

    pub fn tied_to_restoration(&self) -> Option<RestorationId> {
        match self {
            Mutation::Restoration(mutation) => mutation.tied_to_restoration(),
            _ => None,
        }
    }
}

impl Gist for Mutation {
    fn gist(&self) -> String {
        crate::Kind::kind(self).into()
    }
}

#[derive(Clone, Debug)]
#[must_use]
pub enum CoordinatorSend {
    ToDevice {
        message: CoordinatorToDeviceMessage,
        destinations: BTreeSet<DeviceId>,
    },
    ToUser(CoordinatorToUserMessage),
}

#[derive(Debug, Clone)]
pub struct VerifyAddress {
    pub master_appkey: MasterAppkey,
    pub derivation_index: u32,
    pub target_devices: BTreeSet<DeviceId>,
}

impl IntoIterator for VerifyAddress {
    type Item = CoordinatorSend;
    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::VerifyAddress {
                master_appkey: self.master_appkey,
                derivation_index: self.derivation_index,
            },
            destinations: self.target_devices,
        })
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct StartSign {
    pub nonces: BTreeMap<DeviceId, CoordNonceStreamState>,
    pub group_request: GroupSignReq,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestDeviceSign {
    pub request_sign: RequestSign,
    pub device_id: DeviceId,
}

impl From<RequestDeviceSign> for CoordinatorSend {
    fn from(value: RequestDeviceSign) -> Self {
        CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::RequestSign(Box::new(value.request_sign)),
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

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SendDokeygen(pub DoKeyGen);

impl IntoIterator for SendDokeygen {
    type Item = CoordinatorSend;
    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(CoordinatorSend::ToDevice {
            destinations: self.0.device_to_share_index.keys().cloned().collect(),
            message: CoordinatorToDeviceMessage::DoKeyGen(self.0),
        })
    }
}
