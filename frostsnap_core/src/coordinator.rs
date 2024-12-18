use crate::{
    device::KeyPurpose,
    message::*,
    symmetric_encryption::{Ciphertext, SymmetricKey},
    tweak::Xpub,
    AccessStructureId, AccessStructureRef, ActionError, CoordShareDecryptionContrib, Error, Gist,
    KeyId, MasterAppkey, MessageResult, SessionHash, ShareImage, SignItem, SignTask, SignTaskError,
};
use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::{String, ToString},
    vec::Vec,
};
use core::num::NonZeroU32;
use schnorr_fun::{
    frost::{
        self, chilldkg::encpedpop, CoordinatorSignSession, Frost, Nonce, PartyIndex, SharedKey,
    },
    fun::{poly, prelude::*},
    Schnorr, Signature,
};
use sha2::Sha256;
use std::collections::HashMap;
use tracing::{event, Level};

use crate::DeviceId;

pub const MIN_NONCES_BEFORE_REQUEST: usize = 5;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrostCoordinator {
    keys: BTreeMap<KeyId, CoordFrostKey>,
    key_order: Vec<KeyId>,
    action_state: Option<CoordinatorState>,
    device_nonces: HashMap<DeviceId, DeviceNonces>,
    mutations: VecDeque<Mutation>,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CoordFrostKey {
    pub key_id: KeyId,
    pub complete_key: Option<CompleteKey>,
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub recovering_access_structures: HashMap<AccessStructureId, RecoveringAccessStructure>,
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
        encryption_key: SymmetricKey,
    ) -> Option<CoordShareDecryptionContrib> {
        let root_shared_key = self.root_shared_key(access_structure_id, encryption_key)?;
        Some(CoordShareDecryptionContrib::from_root_shared_key(
            &root_shared_key,
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

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RecoveringAccessStructure {
    pub threshold: u16,
    pub share_images: BTreeMap<PartyIndex, (DeviceId, Point<Normal, Public, Zero>)>,
}

macro_rules! fail {
    ($($fail:tt)*) => {{
        tracing::event!(
            tracing::Level::ERROR,
            $($fail)*
        );
        debug_assert!(false, $($fail)*);
    }};
}

impl CoordFrostKey {
    pub fn get_access_structure(
        &self,
        access_structure_id: AccessStructureId,
    ) -> Option<CoordAccessStructure> {
        self.complete_key
            .as_ref()?
            .access_structures
            .get(&access_structure_id)
            .cloned()
    }

    pub fn access_structures(
        &self,
    ) -> impl Iterator<Item = (AccessStructureRef, CoordAccessStructure)> + '_ {
        self.complete_key.iter().flat_map(|completed_key| {
            completed_key.access_structures.iter().map(
                |(&access_structure_id, access_structure)| {
                    (
                        AccessStructureRef {
                            key_id: self.key_id,
                            access_structure_id,
                        },
                        access_structure.clone(),
                    )
                },
            )
        })
    }
}

impl FrostCoordinator {
    pub fn new() -> Self {
        Self {
            keys: Default::default(),
            key_order: Default::default(),
            action_state: None,
            device_nonces: Default::default(),
            mutations: Default::default(),
        }
    }

    pub fn mutate(&mut self, mutation: Mutation) {
        event!(Level::DEBUG, gist = mutation.gist(), "mutating");
        self.apply_mutation(&mutation);
        self.mutations.push_back(mutation);
    }

    pub fn apply_mutation(&mut self, mutation: &Mutation) {
        fn ensure_key<'a>(
            coord: &'a mut FrostCoordinator,
            key_id: KeyId,
            key_name: &str,
            purpose: KeyPurpose,
        ) -> &'a mut CoordFrostKey {
            let exists = coord.keys.contains_key(&key_id);
            let key = coord.keys.entry(key_id).or_insert_with(|| CoordFrostKey {
                key_id,
                complete_key: Default::default(),
                key_name: key_name.to_owned(),
                recovering_access_structures: Default::default(),
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
                key_id,
                key_name,
                purpose,
            } => {
                ensure_key(self, *key_id, key_name, *purpose);
            }
            CompleteKey {
                master_appkey,
                encrypted_rootkey,
            } => {
                if let Some(key) = self.keys.get_mut(&master_appkey.key_id()) {
                    if key.complete_key.is_none() {
                        key.complete_key = Some(self::CompleteKey {
                            master_appkey: *master_appkey,
                            encrypted_rootkey: *encrypted_rootkey,
                            access_structures: Default::default(),
                        });
                    } else {
                        fail!("key is completed");
                    }
                } else {
                    fail!("can't complete non existent key");
                }
            }
            NoncesUsed {
                device_id,
                nonce_counter,
            } => {
                let device_nonces = self.device_nonces.entry(*device_id).or_default();
                debug_assert!(
                    *nonce_counter > device_nonces.start_index,
                    "NoncesUsed should use nonces but  nonce_counter={nonce_counter} <= start_index={}",
                    device_nonces.start_index
                );

                let new_start_index = device_nonces.start_index.max(*nonce_counter);

                while device_nonces.start_index < new_start_index {
                    device_nonces
                        .nonces
                        .pop_front()
                        .expect("NoncesUsed is invalid");
                    device_nonces.start_index += 1;
                }
            }
            ResetNonces { device_id, nonces } => {
                self.device_nonces.insert(*device_id, nonces.clone());
            }
            NewNonces {
                device_id,
                new_nonces,
            } => {
                let device_nonces = self.device_nonces.entry(*device_id).or_default();
                device_nonces.nonces.extend(new_nonces);
            }
            NewAccessStructure { shared_key } => {
                let access_structure_id =
                    AccessStructureId::from_app_poly(shared_key.key().point_polynomial());
                let appkey = MasterAppkey::from_xpub_unchecked(shared_key);
                let key_id = appkey.key_id();
                let access_structure_ref = AccessStructureRef {
                    key_id,
                    access_structure_id,
                };

                match self.keys.get_mut(&key_id) {
                    Some(key_data) => {
                        match key_data.complete_key.as_mut() {
                            Some(complete_key) => {
                                key_data
                                    .recovering_access_structures
                                    .remove(&access_structure_ref.access_structure_id);
                                complete_key.access_structures.insert(
                                    access_structure_id,
                                    CoordAccessStructure {
                                        app_shared_key: shared_key.clone(),
                                        device_to_share_index: Default::default(),
                                    },
                                );
                            }
                            None => {
                                fail!("can't add access structure to incomplete key");
                            }
                        };
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
                    let complete_key = match key_data.complete_key.as_mut() {
                        Some(complete_key) => complete_key,
                        None => {
                            fail!("tried to add a share to incomplete key");
                            return;
                        }
                    };

                    match complete_key
                        .access_structures
                        .get_mut(&access_structure_ref.access_structure_id)
                    {
                        Some(access_structure) => {
                            access_structure
                                .device_to_share_index
                                .insert(*device_id, *share_index);
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
            RecoverShare(self::RecoverShare {
                held_share,
                held_by,
            }) => {
                let key_id = held_share.access_structure_ref.key_id;
                let key = ensure_key(self, key_id, &held_share.key_name, held_share.purpose);
                if key.complete_key.is_some() {
                    self.apply_mutation(&NewShare {
                        access_structure_ref: held_share.access_structure_ref,
                        device_id: *held_by,
                        share_index: held_share.share_image.share_index,
                    });
                } else {
                    let pending_as = key
                        .recovering_access_structures
                        .entry(held_share.access_structure_ref.access_structure_id)
                        .or_insert_with(|| RecoveringAccessStructure {
                            threshold: held_share.threshold,
                            share_images: Default::default(),
                        });
                    pending_as.share_images.insert(
                        held_share.share_image.share_index,
                        (*held_by, held_share.share_image.point),
                    );
                }
            }
            DeleteKey(key_id) => {
                self.keys.remove(key_id);
                self.key_order.retain(|entry| entry != key_id);
            }
        }
    }

    pub fn take_staged_mutations(&mut self) -> VecDeque<Mutation> {
        core::mem::take(&mut self.mutations)
    }

    pub fn staged_mutations(&self) -> &VecDeque<Mutation> {
        &self.mutations
    }

    pub fn restore_sign_session(&mut self, sign_state: SigningSessionState) {
        self.action_state = Some(CoordinatorState::Signing { sign_state });
    }

    pub fn iter_keys(&self) -> impl Iterator<Item = &CoordFrostKey> + '_ {
        self.key_order
            .iter()
            .map(|key_id| self.keys.get(key_id).expect("invariant"))
    }

    pub fn iter_access_structures(
        &self,
    ) -> impl Iterator<Item = (AccessStructureRef, CoordAccessStructure)> + '_ {
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
        match (&mut self.action_state, message) {
            (_, DeviceToCoordinatorMessage::NonceResponse(device_nonces)) => {
                self.mutate(Mutation::ResetNonces {
                    device_id: from,
                    nonces: device_nonces,
                });
                Ok(vec![])
            }
            (_, DeviceToCoordinatorMessage::HeldShares(held_shares)) => {
                let mut messages = vec![];
                for held_share in held_shares {
                    let access_structure_ref = held_share.access_structure_ref;
                    let already_got = self
                        .keys
                        .get(&access_structure_ref.key_id)
                        .map(|coord_key| {
                            let access_structure_id = access_structure_ref.access_structure_id;
                            let is_recovering = coord_key
                                .recovering_access_structures
                                .get(&access_structure_id)
                                .and_then(|recovering_accs| {
                                    recovering_accs
                                        .share_images
                                        .get(&held_share.share_image.share_index)
                                })
                                .is_some();

                            let already_recovered = coord_key
                                .get_access_structure(access_structure_id)
                                .map(|access_structure| access_structure.contains_device(from))
                                .unwrap_or(false);

                            already_recovered || is_recovering
                        })
                        .unwrap_or(false);

                    if !already_got {
                        messages.push(CoordinatorSend::ToUser(
                            CoordinatorToUserMessage::PromptRecoverShare(Box::new(RecoverShare {
                                held_by: from,
                                held_share: held_share.clone(),
                            })),
                        ));
                    }
                }
                Ok(messages)
            }
            (
                Some(CoordinatorState::KeyGen(KeyGenState::WaitingForResponses {
                    input_aggregator,
                    device_to_share_index,
                    pending_key_name,
                    purpose,
                })),
                DeviceToCoordinatorMessage::KeyGenResponse(new_shares),
            ) => {
                let share_index =
                    device_to_share_index
                        .get(&from)
                        .ok_or(Error::coordinator_invalid_message(
                            message_kind,
                            "got share from device that was not part of keygen",
                        ))?;

                input_aggregator
                    .add_input(
                        &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                        // we use the share index as the input generator index. The input
                        // generator at index 0 is the coordinator itself.
                        (*share_index).into(),
                        new_shares,
                    )
                    .map_err(|e| Error::coordinator_invalid_message(message_kind, e))?;

                let mut outgoing = vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                    CoordinatorToUserKeyGenMessage::ReceivedShares { from },
                ))];

                if input_aggregator.is_finished() {
                    let agg_input = input_aggregator.clone().finish().unwrap();
                    let session_hash = SessionHash::from_agg_input(&agg_input);
                    outgoing.push(CoordinatorSend::ToDevice {
                        destinations: device_to_share_index.keys().cloned().collect(),
                        message: CoordinatorToDeviceMessage::FinishKeyGen {
                            agg_input: agg_input.clone(),
                        },
                    });

                    outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                        CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash },
                    )));

                    self.action_state =
                        Some(CoordinatorState::KeyGen(KeyGenState::WaitingForAcks {
                            agg_input: agg_input.clone(),
                            device_to_share_index: device_to_share_index
                                .clone()
                                .into_iter()
                                .map(|(device, share_index)| {
                                    (device, PartyIndex::from(share_index))
                                })
                                .collect(),
                            acks: Default::default(),
                            pending_key_name: pending_key_name.clone(),
                            purpose: *purpose,
                        }));
                }

                Ok(outgoing)
            }
            (
                Some(CoordinatorState::KeyGen(KeyGenState::WaitingForAcks {
                    device_to_share_index,
                    agg_input,
                    acks,
                    ..
                })),
                DeviceToCoordinatorMessage::KeyGenAck(acked_session_hash),
            ) => {
                let mut outgoing = vec![];
                let session_hash = SessionHash::from_agg_input(agg_input);

                if acked_session_hash != session_hash {
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

                    outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                        CoordinatorToUserKeyGenMessage::KeyGenAck {
                            from,
                            all_acks_received,
                        },
                    )));
                }

                Ok(outgoing)
            }
            (
                Some(CoordinatorState::Signing { sign_state }),
                DeviceToCoordinatorMessage::SignatureShare {
                    ref signature_shares,
                    ref mut new_nonces,
                },
            ) => {
                let sessions = &mut sign_state.sessions;
                let n_signatures = sessions.len();
                let frost = frost::new_without_nonce_generation::<Sha256>();
                let mut outgoing = vec![];

                let signer_index = sign_state
                    .access_structure
                    .device_to_share_index
                    .get(&from)
                    .expect("we don't know this device");

                let nonce_for_device =
                    self.device_nonces
                        .get(&from)
                        .ok_or(Error::coordinator_invalid_message(
                            message_kind,
                            "Signer is unknown",
                        ))?;

                // If there have been uncompleted sign requests, the device should replenish more
                // nonces than required for this particular signing session.
                if new_nonces.nonces.len() < n_signatures {
                    return Err(Error::coordinator_invalid_message(
                        message_kind,
                        format!(
                            "Signer did not replenish enough nonces. Expected {n_signatures}, got {}",
                            new_nonces.nonces.len()
                        ),
                    ));
                }

                if signature_shares.len() != n_signatures {
                    return Err(Error::coordinator_invalid_message(message_kind, format!("signer did not provide the right number of signature shares. Got {}, expected {}", signature_shares.len(), sessions.len())));
                }

                // first we do a validation loop and then another loop to actually insert the
                // signature shares so that we don't mutate self unless the entire message is
                // valid.
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
                    CoordinatorToUserSigningMessage::GotShare { from },
                )));

                for (session_progress, signature_share) in sessions.iter_mut().zip(signature_shares)
                {
                    session_progress
                        .signature_shares
                        .insert(from, *signature_share);
                }

                let all_finished = sessions.iter().all(|session| {
                    session.signature_shares.len() == sign_state.access_structure.threshold()
                });

                // the coordinator may want to persist this so a signing session can be restored
                outgoing.push(CoordinatorSend::SigningSessionStore(sign_state.clone()));

                if all_finished {
                    let sessions = &sign_state.sessions;

                    let signatures = sessions
                        .iter()
                        .map(|session_progress| {
                            let sig = session_progress.sign_session.combine_signature_shares(
                                session_progress.sign_session.final_nonce(),
                                session_progress
                                    .signature_shares
                                    .iter()
                                    .map(|(_, &share)| share)
                            );

                            assert!(session_progress.verify_final_signature(
                                &frost.schnorr,
                                &sig,
                            ), "we have verified the signature shares so combined should be correct");

                            sig
                        })
                        .map(EncodedSignature::new)
                        .collect();

                    self.action_state = None;

                    outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::Signing(
                        CoordinatorToUserSigningMessage::Signed { signatures },
                    )));
                }

                if new_nonces.start_index == nonce_for_device.replenish_start() {
                    self.mutate(Mutation::NewNonces {
                        new_nonces: new_nonces.nonces.iter().cloned().collect(),
                        device_id: from,
                    });
                } else {
                    fail!("replenishment nonces returned by device were at the wrong index, got {}, expected {}",
                          new_nonces.start_index, nonce_for_device.replenish_start());
                }

                Ok(outgoing)
            }
            (state, DeviceToCoordinatorMessage::DisplayBackupConfirmed) => {
                if let Some(CoordinatorState::DisplayBackup) = state {
                    Ok(vec![CoordinatorSend::ToUser(
                        CoordinatorToUserMessage::DisplayBackupConfirmed { device_id: from },
                    )])
                } else {
                    // it's ok if a device acks a display backup after we're no longer looking at it
                    // (it shouldn't happen unless the user is trying to make it happen!).
                    Ok(vec![])
                }
            }
            (
                Some(CoordinatorState::CheckingDeviceShare {
                    expected_image,
                    device,
                }),
                DeviceToCoordinatorMessage::CheckShareBackup { share_image },
            ) => {
                if from != *device {
                    return Err(Error::coordinator_invalid_message(
                        message_kind,
                        "unexpected device responded with backup",
                    ));
                }

                Ok(vec![CoordinatorSend::ToUser(
                    CoordinatorToUserMessage::EnteredBackup {
                        device_id: from,
                        valid: *expected_image == share_image,
                    },
                )])
            }
            _ => Err(Error::coordinator_message_kind(
                &self.action_state,
                message_kind,
            )),
        }
    }

    pub fn do_keygen(
        &mut self,
        devices: &BTreeSet<DeviceId>,
        threshold: u16,
        key_name: String,
        key_purpose: KeyPurpose,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<Vec<CoordinatorSend>, ActionError> {
        if devices.len() < threshold as usize {
            panic!(
                "caller needs to ensure that threshold < devices.len(). Tried {}-of-{}",
                threshold,
                devices.len()
            );
        }
        match &self.action_state {
            None => {
                let device_to_share_index: BTreeMap<_, _> = devices
                    .iter()
                    .enumerate()
                    .map(|(index, device_id)| {
                        (
                            *device_id,
                            NonZeroU32::new(index as u32 + 1).expect("we added one"),
                        )
                    })
                    .collect();
                let share_receivers_enckeys = device_to_share_index
                    .iter()
                    .map(|(device, share_index)| (PartyIndex::from(*share_index), device.pubkey()))
                    .collect::<BTreeMap<_, _>>();
                let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
                let mut input_aggregator = encpedpop::Coordinator::new(
                    threshold.into(),
                    (devices.len() + 1) as u32,
                    &share_receivers_enckeys,
                );
                // We don't need to keep the _coordinator_inputter state since we are the one forming agg_input
                //
                let (_coordinator_inputter, input) = encpedpop::Contributor::gen_keygen_input(
                    &schnorr,
                    threshold.into(),
                    &share_receivers_enckeys,
                    0,
                    rng,
                );
                input_aggregator
                    .add_input(&schnorr, 0, input)
                    .expect("we just generated the input");

                self.action_state =
                    Some(CoordinatorState::KeyGen(KeyGenState::WaitingForResponses {
                        input_aggregator,
                        device_to_share_index: device_to_share_index.clone(),
                        pending_key_name: key_name.to_string(),
                        purpose: key_purpose,
                    }));

                Ok(vec![CoordinatorSend::ToDevice {
                    message: CoordinatorToDeviceMessage::DoKeyGen {
                        device_to_share_index,
                        threshold,
                        key_name,
                        purpose: key_purpose,
                    },
                    destinations: devices.clone(),
                }])
            }
            Some(action_state) => Err(ActionError::WrongState {
                in_state: action_state.name(),
                action: "do_keygen",
            }),
        }
    }

    /// This is called when the user has checked every device agrees and finally confirms this with
    /// the coordinator.
    pub fn final_keygen_ack(
        &mut self,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<AccessStructureRef, ActionError> {
        match &self.action_state {
            Some(CoordinatorState::KeyGen(KeyGenState::WaitingForAcks {
                device_to_share_index,
                agg_input,
                acks,
                pending_key_name,
                purpose,
            })) => {
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
                    self.action_state = None;
                    Ok(access_structure_ref)
                } else {
                    Err(ActionError::StateInconsistent(
                        "all device acks have not been received yet".into(),
                    ))
                }
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state_name(),
                action: "final_keygen_ack",
            }),
        }
    }

    pub fn start_sign(
        &mut self,
        access_structure_ref: AccessStructureRef,
        sign_task: SignTask,
        signing_parties: BTreeSet<DeviceId>,
        encryption_key: SymmetricKey,
    ) -> Result<StartSign, StartSignError> {
        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;
        if self.action_state.is_some() {
            // we're doing something else so it's an error to call this
            return Err(StartSignError::CantSignInState {
                in_state: self.state_name(),
            });
        }

        let key_data = self
            .keys
            .get(&key_id)
            .ok_or(StartSignError::UnknownKey { key_id })?
            .clone();

        let complete_key = key_data
            .complete_key
            .as_ref()
            .ok_or(StartSignError::KeyIsRecovering { key_id })?;

        let access_structure = complete_key
            .access_structures
            .get(&access_structure_id)
            .ok_or(StartSignError::NoSuchAccessStructure)?;
        let app_shared_key = access_structure.app_shared_key().clone();

        let root_shared_key = complete_key
            .root_shared_key(access_structure_id, encryption_key)
            .ok_or(StartSignError::CouldntDecryptRootKey)?;

        let selected = signing_parties.len();
        if selected < access_structure.threshold() {
            return Err(StartSignError::NotEnoughDevicesSelected {
                selected,
                threshold: access_structure.threshold(),
            });
        }

        let checked_sign_task = sign_task
            .check(complete_key.master_appkey)
            .map_err(StartSignError::SignTask)?;

        let sign_items = checked_sign_task.sign_items();
        let n_signatures = sign_items.len();

        // For the ToDevice message
        let mut signing_nonces = BTreeMap::default();
        let mut mutations = vec![];

        for &device_id in &signing_parties {
            let share_index = access_structure
                .device_to_share_index
                .get(&device_id)
                .ok_or(StartSignError::DeviceNotPartOfKey { device_id })?;
            let nonces_for_device = match self.device_nonces.get(&device_id) {
                Some(nonces_for_device) => nonces_for_device,
                None => {
                    return Err(StartSignError::NotEnoughNoncesForDevice {
                        device_id,
                        have: 0,
                        need: n_signatures,
                    })
                }
            };

            let index_of_first_nonce = nonces_for_device.start_index;

            if nonces_for_device.nonces.len() < n_signatures {
                return Err(StartSignError::NotEnoughNoncesForDevice {
                    device_id,
                    have: nonces_for_device.nonces.len(),
                    need: n_signatures,
                });
            }

            let nonces_remaining = (nonces_for_device.nonces.len() - n_signatures) as u64;

            let nonces = (0..n_signatures)
                .map(|i| nonces_for_device.nonces[i])
                .collect();

            let new_nonce_counter = index_of_first_nonce
                .checked_add(n_signatures as u64)
                .expect("TODO: guarantee malicious device can't overflow this");

            mutations.push(Mutation::NoncesUsed {
                device_id,
                nonce_counter: new_nonce_counter,
            });

            signing_nonces.insert(
                *share_index,
                SignRequestNonces {
                    nonces,
                    start: index_of_first_nonce,
                    nonces_remaining,
                },
            );
        }

        let frost = frost::new_without_nonce_generation::<Sha256>();

        let sessions = sign_items
            .iter()
            .enumerate()
            .map(|(i, sign_item)| {
                let indexed_nonces = signing_nonces
                    .iter()
                    .map(|(index, sign_req_nonces)| (*index, sign_req_nonces.nonces[i]))
                    .collect();

                SignSessionProgress::new(
                    &frost,
                    app_shared_key.clone(),
                    sign_item.clone(),
                    indexed_nonces,
                )
            })
            .collect();

        let sign_request = SignRequest {
            sign_task: checked_sign_task.into_inner(),
            nonces: signing_nonces.clone(),
            access_structure_id,
            rootkey: root_shared_key.public_key(),
            coord_share_decryption_contrib: CoordShareDecryptionContrib::from_root_shared_key(
                &root_shared_key,
            ),
        };

        // Finally apply the mutations which will extinguish the nonces for the devices
        for mutation in mutations {
            self.mutate(mutation);
        }

        self.action_state = Some(CoordinatorState::Signing {
            sign_state: SigningSessionState {
                targets: signing_parties.clone(),
                sessions,
                request: sign_request.clone(),
                access_structure: access_structure.clone(),
            },
        });

        Ok(StartSign {
            target_devices: signing_parties,
            sign_request,
        })
    }

    pub fn maybe_request_nonce_replenishment(
        &self,
        device_id: DeviceId,
    ) -> Option<CoordinatorSend> {
        let needs_replenishment = match self.device_nonces.get(&device_id) {
            Some(device_nonces) => device_nonces.nonces.len() < MIN_NONCES_BEFORE_REQUEST,
            None => true,
        };

        if needs_replenishment {
            Some(CoordinatorSend::ToDevice {
                message: CoordinatorToDeviceMessage::RequestNonces,
                destinations: FromIterator::from_iter([device_id]),
            })
        } else {
            None
        }
    }

    pub fn request_device_display_backup(
        &mut self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
    ) -> Result<Vec<CoordinatorSend>, ActionError> {
        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;
        let complete_key = self
            .keys
            .get(&key_id)
            .ok_or(ActionError::StateInconsistent("no such key".into()))?
            .complete_key
            .as_ref()
            .ok_or(ActionError::StateInconsistent(
                "can't show backup on key that hasn't been recovered yet".into(),
            ))?;
        let access_structure = complete_key
            .access_structures
            .get(&access_structure_id)
            .ok_or(ActionError::StateInconsistent(
                "no such access structure".into(),
            ))?;
        let party_index = *access_structure
            .device_to_share_index
            .get(&device_id)
            .ok_or(ActionError::StateInconsistent(
                "device does not have share in key".into(),
            ))?;
        self.action_state = Some(CoordinatorState::DisplayBackup);
        let rootkey = complete_key
            .encrypted_rootkey
            .decrypt(encryption_key)
            .ok_or(ActionError::StateInconsistent(
                "couldn't decrypt root key".into(),
            ))?;
        let coord_share_decryption_contrib = complete_key
            .coord_share_decryption_contrib(access_structure_id, encryption_key)
            .ok_or(ActionError::StateInconsistent(
                "couldn't decrypt root key".into(),
            ))?;
        Ok(vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::DisplayBackup {
                key_id: KeyId::from_rootkey(rootkey),
                access_structure_id,
                coord_share_decryption_contrib,
                party_index,
            },
            destinations: BTreeSet::from_iter([device_id]),
        }])
    }

    pub fn check_share(
        &mut self,
        access_structure_ref: AccessStructureRef,
        device: DeviceId,
        encryption_key: SymmetricKey,
    ) -> Result<Vec<CoordinatorSend>, ActionError> {
        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;

        let complete_key = self
            .keys
            .get(&key_id)
            .ok_or(ActionError::StateInconsistent("no such key".into()))?
            .complete_key
            .as_ref()
            .ok_or(ActionError::StateInconsistent(
                "can't check share of a key that hasn't been restored".into(),
            ))?
            .clone();

        let root_shared_key = complete_key
            .root_shared_key(access_structure_id, encryption_key)
            .ok_or(ActionError::StateInconsistent(
                "couldn't decrypt root key".into(),
            ))?;

        let access_structure = self.get_access_structure(access_structure_ref).ok_or(
            ActionError::StateInconsistent("no such access_structure".into()),
        )?;

        let share_index = *access_structure.device_to_share_index.get(&device).ok_or(
            ActionError::StateInconsistent("device doesn't own share in access structure".into()),
        )?;

        let expected_image = ShareImage {
            point: poly::point::eval(root_shared_key.point_polynomial(), share_index).normalize(),
            share_index,
        };

        self.action_state = Some(CoordinatorState::CheckingDeviceShare {
            expected_image,
            device,
        });
        Ok(vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::CheckShareBackup,
            destinations: BTreeSet::from_iter([device]),
        }])
    }

    pub fn verify_address(
        &self,
        key_id: KeyId,
        derivation_index: u32,
    ) -> Result<VerifyAddress, ActionError> {
        let frost_key = self
            .get_frost_key(key_id)
            .ok_or(ActionError::StateInconsistent("no such frost key".into()))?;

        let master_appkey = frost_key
            .complete_key
            .as_ref()
            .ok_or(ActionError::StateInconsistent(
                "can't verify address for incomplete key".into(),
            ))?
            .master_appkey;

        // verify on any device that knows about this key
        let target_devices: BTreeSet<_> = frost_key
            .access_structures()
            .flat_map(|(_, accss)| {
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

    pub fn state_name(&self) -> &'static str {
        self.action_state
            .as_ref()
            .map(|x| x.name())
            .unwrap_or("None")
    }

    pub fn cancel(&mut self) {
        let _state = self.action_state.take();
    }

    pub fn device_nonces(&self) -> &HashMap<DeviceId, DeviceNonces> {
        &self.device_nonces
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

    pub fn recover_share(&mut self, recover_share: RecoverShare) -> bool {
        let access_structure_ref = recover_share.held_share.access_structure_ref;
        self.mutate(Mutation::RecoverShare(recover_share));

        let coord_key = self
            .keys
            .get(&access_structure_ref.key_id)
            .expect("mutation should have created it");

        // This may be `None` if the access structure has already been recovered
        if let Some(pending_as) = coord_key
            .recovering_access_structures
            .get(&access_structure_ref.access_structure_id)
        {
            return pending_as.share_images.len() >= pending_as.threshold as usize;
        }

        false
    }

    pub fn recover_share_and_maybe_recover_access_structure(
        &mut self,
        recover_share: RecoverShare,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<(), ActionError> {
        let access_structure_ref = recover_share.held_share.access_structure_ref;
        let ready_for_as_recovery = self.recover_share(recover_share);

        if ready_for_as_recovery {
            self.recover_access_structure(access_structure_ref, encryption_key, rng)?;
        }

        Ok(())
    }

    pub fn recover_access_structure(
        &mut self,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<(), ActionError> {
        let key = self
            .keys
            .get(&access_structure_ref.key_id)
            .ok_or(ActionError::StateInconsistent("key doesn't exist".into()))?;
        let pending_as = key
            .recovering_access_structures
            .get(&access_structure_ref.access_structure_id)
            .ok_or(ActionError::StateInconsistent(format!(
                "access structure for recovery not found {:?}",
                access_structure_ref
            )))?;
        if pending_as.share_images.len() >= pending_as.threshold as usize {
            let share_images = pending_as
                .share_images
                .clone()
                .into_iter()
                .map(|(share_index, (_, share_image))| (share_index, share_image))
                .collect::<Vec<_>>();
            let implied_root_poly = poly::point::interpolate(&share_images);
            let implied_root_poly = poly::point::normalize(implied_root_poly).collect::<Vec<_>>();
            let root_shared_key = SharedKey::from_poly(implied_root_poly).non_zero().ok_or(
                ActionError::StateInconsistent("can't recover a zero key".into()),
            )?;
            let device_to_share_index = pending_as
                .clone()
                .share_images
                .into_iter()
                .map(|(party_index, (device_id, _))| (device_id, party_index))
                .collect();
            self.mutate_new_key(
                key.key_name.clone(),
                root_shared_key,
                device_to_share_index,
                encryption_key,
                key.purpose,
                rng,
            );
            Ok(())
        } else {
            Err(ActionError::StateInconsistent(format!(
                "not enough shares to recover {:?} yet. Have {}, need {}.",
                access_structure_ref,
                pending_as.share_images.len(),
                pending_as.threshold
            )))
        }
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
        let master_appkey = MasterAppkey::derive_from_rootkey(rootkey);
        let key_id = master_appkey.key_id();
        let access_structure_ref = AccessStructureRef {
            key_id,
            access_structure_id,
        };

        if self.get_frost_key(key_id).is_none() {
            self.mutate(Mutation::NewKey {
                key_name: name,
                key_id,
                purpose,
            });
        }

        let frost_key = self.get_frost_key(key_id).expect("just inserted");

        if frost_key.complete_key.is_none() {
            self.mutate(Mutation::CompleteKey {
                master_appkey,
                encrypted_rootkey,
            });
        }

        self.mutate(Mutation::NewAccessStructure {
            shared_key: app_shared_key,
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

    pub fn request_held_shares(&self, id: DeviceId) -> impl Iterator<Item = CoordinatorSend> {
        core::iter::once(CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::RequestHeldShares,
            destinations: [id].into(),
        })
    }
}

impl CoordinatorState {
    pub fn name(&self) -> &'static str {
        match self {
            CoordinatorState::KeyGen(keygen_state) => match keygen_state {
                KeyGenState::WaitingForResponses { .. } => "WaitingForResponses",
                KeyGenState::WaitingForAcks { .. } => "WaitingForAcks",
            },
            CoordinatorState::Signing { .. } => "Signing",
            CoordinatorState::DisplayBackup => "DisplayBackup",
            CoordinatorState::CheckingDeviceShare { .. } => "RestoringDeviceShare",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SignSessionProgress {
    sign_item: SignItem,
    sign_session: CoordinatorSignSession,
    signature_shares: BTreeMap<DeviceId, Scalar<Public, Zero>>,
    app_shared_key: Xpub<SharedKey>,
}

impl SignSessionProgress {
    pub fn new<NG>(
        frost: &Frost<sha2::Sha256, NG>,
        app_shared_key: Xpub<SharedKey>,
        sign_item: SignItem,
        nonces: BTreeMap<frost::PartyIndex, frost::Nonce>,
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

#[derive(Clone, Debug, PartialEq)]
// There's usually only one instance of this enum.
// Having it take up the max space doesn't matter.
#[allow(clippy::large_enum_variant)]
pub enum CoordinatorState {
    KeyGen(KeyGenState),
    Signing {
        sign_state: SigningSessionState,
    },
    DisplayBackup,
    CheckingDeviceShare {
        expected_image: ShareImage,
        device: DeviceId,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SigningSessionState {
    pub targets: BTreeSet<DeviceId>,
    pub sessions: Vec<SignSessionProgress>,
    pub request: SignRequest,
    pub access_structure: CoordAccessStructure,
}

impl SigningSessionState {
    pub fn received_from(&self) -> impl Iterator<Item = DeviceId> + '_ {
        // all sessions make progress at the same time
        self.sessions[0].received_from()
    }

    pub fn session_id(&self) -> [u8; 32] {
        self.request.session_id()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyGenState {
    WaitingForResponses {
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
}

impl CoordAccessStructure {
    pub fn threshold(&self) -> usize {
        self.app_shared_key.key.threshold()
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
    UnknownKey {
        key_id: KeyId,
    },
    DeviceNotPartOfKey {
        device_id: DeviceId,
    },
    NotEnoughDevicesSelected {
        selected: usize,
        threshold: usize,
    },
    CantSignInState {
        in_state: &'static str,
    },
    NotEnoughNoncesForDevice {
        device_id: DeviceId,
        have: usize,
        need: usize,
    },
    SignTask(SignTaskError),
    NoSuchAccessStructure,
    CouldntDecryptRootKey,
    KeyIsRecovering {
        key_id: KeyId,
    },
}

impl core::fmt::Display for StartSignError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
            StartSignError::NotEnoughNoncesForDevice {
                device_id,
                have,
                need,
            } => {
                write!(
                    f,
                    "Not enough nonces for device {}, have {}, need {}",
                    device_id, have, need,
                )
            }
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
            StartSignError::KeyIsRecovering { key_id } => write!(
                f,
                "you can't sign under the key {key_id} that hasn't been fully recovered"
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StartSignError {}

/// Mutations to the coordinator state
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub enum Mutation {
    NewKey {
        key_name: String,
        key_id: KeyId,
        purpose: KeyPurpose,
    },
    CompleteKey {
        master_appkey: MasterAppkey,
        encrypted_rootkey: Ciphertext<33, Point>,
    },
    NewAccessStructure {
        shared_key: Xpub<SharedKey>,
    },
    NoncesUsed {
        device_id: DeviceId,
        /// if nonce_counter = x, then the coordinator expects x to be the next nonce used.
        /// (anything < x has been used)
        nonce_counter: u64,
    },
    ResetNonces {
        device_id: DeviceId,
        nonces: DeviceNonces,
    },
    NewNonces {
        device_id: DeviceId,
        new_nonces: Vec<Nonce>,
    },
    NewShare {
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
        share_index: PartyIndex,
    },
    RecoverShare(RecoverShare),
    DeleteKey(KeyId),
}

impl Mutation {
    pub fn tied_to_key(&self) -> Option<KeyId> {
        Some(match self {
            Mutation::NewKey { key_id, .. } => *key_id,
            Mutation::CompleteKey { master_appkey, .. } => master_appkey.key_id(),
            Mutation::NewAccessStructure { shared_key } => {
                MasterAppkey::from_xpub_unchecked(shared_key).key_id()
            }
            Mutation::NewShare {
                access_structure_ref,
                ..
            } => access_structure_ref.key_id,
            Mutation::RecoverShare(RecoverShare { held_share, .. }) => {
                held_share.access_structure_ref.key_id
            }
            Mutation::DeleteKey(key_id) => *key_id,
            _ => return None,
        })
    }
}

impl Gist for Mutation {
    fn gist(&self) -> String {
        use Mutation::*;
        match self {
            NoncesUsed { .. } => "NoncesUsed",
            CompleteKey { .. } => "CompleteKey",
            ResetNonces { .. } => "ResetNonces",
            NewNonces { .. } => "NewNonces",
            NewAccessStructure { .. } => "NewAccessStructure",
            NewKey { .. } => "NewKey",
            NewShare { .. } => "NewShare",
            RecoverShare { .. } => "RecoverShare",
            DeleteKey(_) => "DeleteKey",
        }
        .into()
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
    SigningSessionStore(SigningSessionState),
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

#[derive(Debug, Clone)]
pub struct StartSign {
    pub target_devices: BTreeSet<DeviceId>,
    pub sign_request: SignRequest,
}

impl IntoIterator for StartSign {
    type Item = CoordinatorSend;
    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::RequestSign(self.sign_request),
            destinations: self.target_devices,
        })
    }
}
