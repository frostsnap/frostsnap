use crate::{
    message::*, ActionError, Error, FrostKeyExt, Gist, KeyId, MessageResult, SignItem, SignTask,
    SignTaskError,
};
use alloc::{
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
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::DeviceId;

pub const MIN_NONCES_BEFORE_REQUEST: usize = 5;

#[derive(Debug, Clone, Default)]
pub struct FrostCoordinator {
    keys: BTreeMap<KeyId, CoordinatorFrostKey>,
    key_order: Vec<KeyId>,
    action_state: Option<CoordinatorState>,
    device_nonces: BTreeMap<DeviceId, DeviceNonces>,
    mutations: VecDeque<Mutation>,
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
        self.apply_mutation(&mutation);
        self.mutations.push_back(mutation);
    }

    pub fn apply_mutation(&mut self, mutation: &Mutation) {
        use Mutation::*;
        match mutation {
            NewKey(key) => {
                let key_id = key.frost_key().key_id();
                let actually_new = self.keys.insert(key_id, key.clone()).is_none();
                if actually_new {
                    self.key_order.push(key_id);
                }
            }
            NoncesUsed {
                device_id,
                nonce_counter,
            } => {
                let device_nonces = self.device_nonces.entry(*device_id).or_default();
                debug_assert!(
                    *nonce_counter > device_nonces.start_index,
                    "NoncesUsed should use nonces"
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
        }
    }

    pub fn take_staged_mutations(&mut self) -> VecDeque<Mutation> {
        core::mem::take(&mut self.mutations)
    }

    pub fn restore_sign_session(&mut self, sign_state: SigningSessionState) {
        let key = self
            .frost_key_state(sign_state.request.key_id)
            .expect("cannot restore in coordinator without state")
            .clone();
        self.action_state = Some(CoordinatorState::Signing { sign_state, key });
    }

    pub fn iter_keys(&self) -> impl Iterator<Item = &CoordinatorFrostKey> + '_ {
        self.key_order
            .iter()
            .map(|key_id| self.keys.get(key_id).expect("invariant"))
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
            (
                Some(CoordinatorState::KeyGen(KeyGenState::WaitingForResponses {
                    input_aggregator,
                    device_to_share_index,
                    pending_key_name,
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
                    let session_hash = Sha256::default()
                        .chain_update(agg_input.cert_bytes())
                        .finalize_fixed()
                        .into();
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
                let session_hash: [u8; 32] = Sha256::default()
                    .chain_update(agg_input.cert_bytes())
                    .finalize_fixed()
                    .into();

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
                Some(CoordinatorState::Signing { key, sign_state }),
                DeviceToCoordinatorMessage::SignatureShare {
                    ref signature_shares,
                    ref mut new_nonces,
                },
            ) => {
                let sessions = &mut sign_state.sessions;
                let n_signatures = sessions.len();
                let frost = frost::new_without_nonce_generation::<Sha256>();
                let mut outgoing = vec![];

                let signer_index = key
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

                let all_finished = sessions
                    .iter()
                    .all(|session| session.signature_shares.len() == key.frost_key.threshold());

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
                    debug_assert!(false, "we shouldn't hit this branch");
                    #[cfg(feature = "tracing")]
                    tracing::event!(
                        tracing::Level::ERROR,
                        got = new_nonces.start_index,
                        expected = nonce_for_device.replenish_start(),
                        "replenishment nonces returned by device were at the wrong index"
                    );
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
                Some(CoordinatorState::LoadingDeviceShare { key, device }),
                DeviceToCoordinatorMessage::CheckShareBackup {
                    share_index,
                    share_image,
                },
            ) => {
                if from != *device {
                    return Err(Error::coordinator_invalid_message(
                        message_kind,
                        "unexpected device responded with backup",
                    ));
                }

                let frost_key = key.frost_key();
                let polynomial = frost_key.point_polynomial();
                let expected = poly::point::eval(polynomial, share_index);

                Ok(vec![CoordinatorSend::ToUser(
                    CoordinatorToUserMessage::EnteredBackup {
                        device_id: from,
                        valid: expected == share_image,
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
                    }));

                Ok(vec![CoordinatorSend::ToDevice {
                    message: CoordinatorToDeviceMessage::DoKeyGen {
                        device_to_share_index,
                        threshold,
                        key_name,
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
    pub fn final_keygen_ack(&mut self) -> Result<KeyId, ActionError> {
        match &self.action_state {
            Some(CoordinatorState::KeyGen(KeyGenState::WaitingForAcks {
                device_to_share_index,
                agg_input,
                acks,
                pending_key_name,
            })) => {
                let all_acks = acks.len() == device_to_share_index.len();
                if all_acks {
                    let key = CoordinatorFrostKey {
                        frost_key: agg_input.shared_key().non_zero().expect("invariant"),
                        device_to_share_index: device_to_share_index.clone(),
                        key_name: pending_key_name.clone(),
                    };
                    let key_id = key.frost_key.key_id();
                    self.action_state = None;

                    self.mutate(Mutation::NewKey(key));
                    Ok(key_id)
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
        key_id: KeyId,
        sign_task: SignTask,
        signing_parties: BTreeSet<DeviceId>,
    ) -> Result<Vec<CoordinatorSend>, StartSignError> {
        if self.action_state.is_some() {
            // we're doing something else so it's an error to call this
            return Err(StartSignError::CantSignInState {
                in_state: self.state_name(),
            });
        }

        let key = self
            .keys
            .get(&key_id)
            .ok_or(StartSignError::UnknownKey { key_id })?
            .clone();
        let frost_key = key.frost_key.clone();
        let selected = signing_parties.len();
        if selected < frost_key.threshold() {
            return Err(StartSignError::NotEnoughDevicesSelected {
                selected,
                threshold: frost_key.threshold(),
            });
        }

        let checked_sign_task = sign_task
            .check(frost_key.key_id())
            .map_err(StartSignError::SignTask)?;

        let sign_items = checked_sign_task.sign_items();
        let n_signatures = sign_items.len();

        // For the ToDevice message
        let mut signing_nonces = BTreeMap::default();
        let mut mutations = vec![];

        for &device_id in &signing_parties {
            let share_index = key
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
                    key.frost_key.clone(),
                    sign_item.clone(),
                    indexed_nonces,
                )
            })
            .collect();

        let sign_request = SignRequest {
            sign_task: checked_sign_task.into_inner(),
            nonces: signing_nonces.clone(),
            key_id,
        };

        // Finally apply the mutations which will extinguish the nonces for the devices
        for mutation in mutations {
            self.mutate(mutation);
        }

        self.action_state = Some(CoordinatorState::Signing {
            key,
            sign_state: SigningSessionState {
                targets: signing_parties.clone(),
                sessions,
                request: sign_request.clone(),
            },
        });

        Ok(vec![CoordinatorSend::ToDevice {
            destinations: signing_parties,
            message: CoordinatorToDeviceMessage::RequestSign(sign_request),
        }])
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
        key_id: KeyId,
    ) -> Result<Vec<CoordinatorSend>, ActionError> {
        let key = self
            .keys
            .get(&key_id)
            .ok_or(ActionError::StateInconsistent("no such key".into()))?;
        let _ = key
            .device_to_share_index
            .get(&device_id)
            .ok_or(ActionError::StateInconsistent(
                "device does not have share in key".into(),
            ))?;
        self.action_state = Some(CoordinatorState::DisplayBackup);
        Ok(vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::DisplayBackup { key_id },
            destinations: BTreeSet::from_iter([device_id]),
        }])
    }

    pub fn check_share(
        &mut self,
        device_id: DeviceId,
        key_id: KeyId,
    ) -> Result<Vec<CoordinatorSend>, ActionError> {
        let key = self
            .keys
            .get(&key_id)
            .ok_or(ActionError::StateInconsistent("no such key".into()))?;
        self.action_state = Some(CoordinatorState::LoadingDeviceShare {
            key: key.clone(),
            device: device_id,
        });
        Ok(vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::CheckShareBackup,
            destinations: BTreeSet::from_iter([device_id]),
        }])
    }

    pub fn state_name(&self) -> &'static str {
        self.action_state
            .as_ref()
            .map(|x| x.name())
            .unwrap_or("None")
    }

    pub fn frost_key_state(&self, key_id: KeyId) -> Option<&CoordinatorFrostKey> {
        self.keys.get(&key_id)
    }

    pub fn cancel(&mut self) {
        let _state = self.action_state.take();
    }

    pub fn device_nonces(&self) -> &BTreeMap<DeviceId, DeviceNonces> {
        &self.device_nonces
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
            CoordinatorState::LoadingDeviceShare { .. } => "RestoringDeviceShare",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SignSessionProgress {
    sign_item: SignItem,
    sign_session: CoordinatorSignSession,
    signature_shares: BTreeMap<DeviceId, Scalar<Public, Zero>>,
    root_key: SharedKey,
}

impl SignSessionProgress {
    pub fn new<NG>(
        frost: &Frost<sha2::Sha256, NG>,
        root_key: SharedKey,
        sign_item: SignItem,
        nonces: BTreeMap<frost::PartyIndex, frost::Nonce>,
    ) -> Self {
        let tweaked_key = sign_item.app_tweak.derive_xonly_key(&root_key);
        let sign_session =
            frost.coordinator_sign_session(&tweaked_key, nonces, sign_item.schnorr_fun_message());
        Self {
            sign_item,
            sign_session,
            signature_shares: Default::default(),
            root_key,
        }
    }

    pub fn received_from(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.signature_shares.keys().cloned()
    }

    pub fn tweaked_frost_key(&self) -> SharedKey<EvenY> {
        self.sign_item.app_tweak.derive_xonly_key(&self.root_key)
    }

    pub fn verify_final_signature<NG>(
        &self,
        schnorr: &Schnorr<sha2::Sha256, NG>,
        signature: &Signature,
    ) -> bool {
        self.sign_item
            .verify_final_signature(schnorr, self.root_key.key_id(), signature)
    }
}

#[derive(Clone, Debug)]
pub enum CoordinatorState {
    KeyGen(KeyGenState),
    Signing {
        sign_state: SigningSessionState,
        key: CoordinatorFrostKey,
    },
    DisplayBackup,
    LoadingDeviceShare {
        key: CoordinatorFrostKey,
        device: DeviceId,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SigningSessionState {
    pub targets: BTreeSet<DeviceId>,
    pub sessions: Vec<SignSessionProgress>,
    pub request: SignRequest,
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

#[derive(Clone, Debug)]
pub enum KeyGenState {
    WaitingForResponses {
        input_aggregator: encpedpop::Coordinator,
        device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
        pending_key_name: String,
    },
    WaitingForAcks {
        agg_input: encpedpop::AggKeygenInput,
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        acks: BTreeSet<DeviceId>,
        pending_key_name: String,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize)]
pub struct CoordinatorFrostKey {
    frost_key: SharedKey,
    device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
    key_name: String,
}

impl CoordinatorFrostKey {
    pub fn new(
        frost_key: SharedKey,
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        key_name: String,
    ) -> Self {
        Self {
            frost_key,
            device_to_share_index,
            key_name,
        }
    }

    pub fn threshold(&self) -> usize {
        self.frost_key.threshold()
    }

    pub fn frost_key(&self) -> SharedKey<Normal> {
        self.frost_key.clone()
    }

    pub fn key_id(&self) -> KeyId {
        self.frost_key.key_id()
    }

    pub fn devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_to_share_index.keys().cloned()
    }

    pub fn key_name(&self) -> String {
        self.key_name.clone()
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
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StartSignError {}

/// Mutations to the coordinator state
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum Mutation {
    NewKey(CoordinatorFrostKey),
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
}

impl Gist for Mutation {
    fn gist(&self) -> String {
        use Mutation::*;
        match self {
            NoncesUsed { .. } => "NoncesUsed",
            ResetNonces { .. } => "ResetNonces",
            NewNonces { .. } => "NewNonces",
            NewKey(_) => "NewKey",
        }
        .into()
    }
}
