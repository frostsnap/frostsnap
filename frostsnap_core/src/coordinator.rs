use crate::{
    gen_pop_message, message::*, ActionError, Error, FrostKeyExt, KeyId, MessageResult, SessionHash,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use schnorr_fun::{
    frost::{self, EncodedFrostKey, Frost, FrostKey, SignSession},
    fun::{marker::*, Scalar},
    Message, Schnorr, Signature,
};
use sha2::Sha256;

use crate::DeviceId;

pub const MIN_NONCES_BEFORE_REQUEST: usize = 5;

#[derive(Debug, Clone, Default)]
pub struct FrostCoordinator {
    keys: BTreeMap<KeyId, CoordinatorFrostKey>,
    key_order: Vec<KeyId>,
    action_state: Option<CoordinatorState>,
    device_nonces: BTreeMap<DeviceId, DeviceNonces>,
}

impl FrostCoordinator {
    pub fn new() -> Self {
        Self {
            keys: Default::default(),
            key_order: Default::default(),
            action_state: None,
            device_nonces: Default::default(),
        }
    }

    pub fn apply_change(&mut self, change: CoordinatorToStorageMessage) {
        use CoordinatorToStorageMessage::*;
        match change {
            NewKey(key) => {
                let key_id = key.frost_key().key_id();
                let actually_new = self.keys.insert(key_id, key).is_none();
                if actually_new {
                    self.key_order.push(key_id);
                }
            }
            NoncesUsed {
                device_id,
                nonce_counter,
            } => {
                let device_nonces = self.device_nonces.entry(device_id).or_default();
                let _nonce = device_nonces
                    .nonces
                    .pop_front()
                    .expect("we need to have had a nonce to apply a NonceUsed change");
                device_nonces.start_index = device_nonces.start_index.max(nonce_counter);
            }
            ResetNonces { device_id, nonces } => {
                self.device_nonces.insert(device_id, nonces);
            }
            NewNonces {
                device_id,
                new_nonces,
            } => {
                let device_nonces = self.device_nonces.entry(device_id).or_default();
                device_nonces.nonces.extend(new_nonces);
            }
            StoreSigningState(sign_state) => {
                let key = self
                    .frost_key_state(sign_state.request.key_id)
                    .expect("cannot restore in coordinator without state")
                    .clone();
                self.action_state = Some(CoordinatorState::Signing { sign_state, key });
            }
        }
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
                self.device_nonces.insert(from, device_nonces.clone());
                Ok(vec![CoordinatorSend::ToStorage(
                    CoordinatorToStorageMessage::ResetNonces {
                        device_id: from,
                        nonces: device_nonces,
                    },
                )])
            }
            (
                Some(CoordinatorState::KeyGen(KeyGenState::WaitingForResponses {
                    device_to_share_index,
                    responses,
                    threshold,
                })),
                DeviceToCoordinatorMessage::KeyGenResponse(new_shares),
            ) => {
                if let Some(existing) = responses.insert(from, Some(new_shares.clone())) {
                    debug_assert!(existing.is_none(), "Device sent keygen response twice");
                }

                if new_shares.my_poly.len() != *threshold as usize {
                    return Err(Error::coordinator_invalid_message(
                        message_kind,
                        "Device sent polynomial with incorrect threshold",
                    ));
                }

                let mut outgoing = vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                    CoordinatorToUserKeyGenMessage::ReceivedShares { from },
                ))];

                let all_responded = responses
                    .clone()
                    .into_iter()
                    .map(|(device_id, shares)| Some((device_id, shares?)))
                    .collect::<Option<BTreeMap<_, _>>>();

                match all_responded {
                    Some(responses) => {
                        let point_polys = responses
                            .iter()
                            .map(|(device_id, response)| {
                                (
                                    *device_to_share_index
                                        .get(device_id)
                                        .expect("this device is a part of keygen"),
                                    response.my_poly.clone(),
                                )
                            })
                            .collect();
                        let proofs_of_possession = responses
                            .iter()
                            .map(|(device_id, response)| {
                                (
                                    *device_to_share_index
                                        .get(device_id)
                                        .expect("this device is a part of keygen"),
                                    response.proof_of_possession.clone(),
                                )
                            })
                            .collect();
                        let frost = frost::new_without_nonce_generation::<Sha256>();
                        let keygen = frost
                            .new_keygen::<&[Scalar]>(point_polys, &BTreeMap::new())
                            .unwrap();
                        // let keygen_id = frost.keygen_id(&keygen);
                        let pop_message = gen_pop_message(responses.keys().cloned());

                        let frost_key = match frost.finish_keygen_coordinator(keygen, proofs_of_possession, Message::raw(&pop_message)) {
                            Ok(frost_key) => frost_key,
                            Err(_) => todo!("should notify user somehow that everything was fucked and we're canceling it"),
                        };

                        // TODO: This is definitely insufficient
                        let session_hash = frost_key
                            .clone()
                            .into_xonly_key()
                            .public_key()
                            .to_xonly_bytes();

                        self.action_state =
                            Some(CoordinatorState::KeyGen(KeyGenState::WaitingForAcks {
                                device_to_share_index: device_to_share_index.clone(),
                                frost_key: frost_key.into(),
                                acks: responses
                                    .clone()
                                    .into_keys()
                                    .map(|id| (id, false))
                                    .collect(),
                                session_hash,
                            }));

                        // TODO: check order
                        outgoing.push(CoordinatorSend::ToDevice {
                            destinations: responses.keys().cloned().collect(),
                            message: CoordinatorToDeviceMessage::FinishKeyGen {
                                shares_provided: responses,
                            },
                        });
                        outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                            CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash },
                        )));
                    }
                    None => { /* not finished yet  */ }
                };
                Ok(outgoing)
            }
            (
                Some(CoordinatorState::KeyGen(KeyGenState::WaitingForAcks {
                    device_to_share_index,
                    frost_key,
                    acks,
                    session_hash,
                })),
                DeviceToCoordinatorMessage::KeyGenAck(acked_session_hash),
            ) => {
                let mut outgoing = vec![];
                if acked_session_hash != *session_hash {
                    return Err(Error::coordinator_invalid_message(
                        message_kind,
                        "Device acked wrong keygen session hash",
                    ));
                }

                match acks.get_mut(&from) {
                    None => {
                        return Err(Error::coordinator_invalid_message(
                            message_kind,
                            "Received ack from device not a member of keygen",
                        ));
                    }
                    Some(ack) => {
                        outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                            CoordinatorToUserKeyGenMessage::KeyGenAck { from },
                        )));
                        *ack = true;
                    }
                }

                let all_acks = acks.values().all(|ack| *ack);
                if all_acks {
                    let key = CoordinatorFrostKey {
                        encoded_frost_key: frost_key.clone(),
                        device_to_share_index: device_to_share_index.clone(),
                    };
                    let key_id = key.encoded_frost_key.into_frost_key().key_id();
                    self.action_state = None;

                    let change = CoordinatorToStorageMessage::NewKey(key);
                    self.apply_change(change.clone());
                    outgoing.extend([
                        CoordinatorSend::ToStorage(change),
                        CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                            CoordinatorToUserKeyGenMessage::FinishedKey { key_id },
                        )),
                    ]);
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

                let from_share_index = key
                    .device_to_share_index
                    .get(&from)
                    .expect("we don't know this device");

                let nonce_for_device =
                    self.device_nonces
                        .get_mut(&from)
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
                    if !session
                        .participants()
                        .any(|x_coord| x_coord == *from_share_index)
                    {
                        return Err(Error::coordinator_invalid_message(
                            message_kind,
                            "Signer was not a particpant for this session",
                        ));
                    }

                    if !frost.verify_signature_share(
                        xonly_frost_key,
                        session,
                        *from_share_index,
                        *signature_share,
                    ) {
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
                    session.signature_shares.len()
                        == key.encoded_frost_key.into_frost_key().threshold()
                });

                // the coordinator may want to persist this so a signing session can be restored
                outgoing.push(CoordinatorSend::ToStorage(
                    CoordinatorToStorageMessage::StoreSigningState(sign_state.clone()),
                ));

                if all_finished {
                    let sessions = &sign_state.sessions;

                    let signatures = sessions
                        .iter()
                        .map(|session_progress| {
                            let xonly_frost_key = session_progress.tweaked_frost_key();

                            let sig = frost.combine_signature_shares(
                                &xonly_frost_key,
                                &session_progress.sign_session,
                                session_progress
                                    .signature_shares
                                    .iter()
                                    .map(|(_, &share)| share)
                                    .collect(),
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
                    outgoing.push(CoordinatorSend::ToStorage(
                        CoordinatorToStorageMessage::NewNonces {
                            new_nonces: new_nonces.nonces.iter().cloned().collect(),
                            device_id: from,
                        },
                    ));
                    nonce_for_device.nonces.append(&mut new_nonces.nonces);
                } else {
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
                            Scalar::from((index + 1) as u32).non_zero().unwrap(),
                        )
                    })
                    .collect();

                self.action_state =
                    Some(CoordinatorState::KeyGen(KeyGenState::WaitingForResponses {
                        device_to_share_index: device_to_share_index.clone(),
                        responses: devices.iter().map(|&device_id| (device_id, None)).collect(),
                        threshold,
                    }));

                Ok(vec![CoordinatorSend::ToDevice {
                    message: CoordinatorToDeviceMessage::DoKeyGen {
                        device_to_share_index,
                        threshold,
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
            .ok_or(StartSignError::UnknownKey { key_id })?;
        let frost_key = key.encoded_frost_key.into_frost_key();
        let selected = signing_parties.len();
        if selected < frost_key.threshold() {
            return Err(StartSignError::NotEnoughDevicesSelected {
                selected,
                threshold: frost_key.threshold(),
            });
        }

        let sign_items = sign_task.sign_items();
        let n_signatures = sign_items.len();

        // For the ToDevice message
        let mut signing_nonces = BTreeMap::default();
        // ToStorage messages so we persist which nonces we're usign
        let mut used_nonces = vec![];

        for &device_id in &signing_parties {
            let share_index = *key
                .device_to_share_index
                .get(&device_id)
                .ok_or(StartSignError::DeviceNotPartOfKey { device_id })?;
            let nonces_for_device = self.device_nonces.entry(device_id).or_default();
            let index_of_first_nonce = nonces_for_device.start_index;
            if nonces_for_device.nonces.len() < n_signatures {
                return Err(StartSignError::NotEnoughNoncesForDevice {
                    device_id,
                    have: nonces_for_device.nonces.len(),
                    need: n_signatures,
                });
            }
            let nonces = core::iter::from_fn(|| {
                let nonce = nonces_for_device.nonces.pop_front()?;
                nonces_for_device.start_index += 1;
                Some(nonce)
            })
            .take(n_signatures)
            .collect();

            signing_nonces.insert(
                share_index,
                SignRequestNonces {
                    nonces,
                    start: index_of_first_nonce,
                    nonces_remaining: nonces_for_device.nonces.len() as u64,
                },
            );
            used_nonces.push(CoordinatorToStorageMessage::NoncesUsed {
                device_id,
                nonce_counter: nonces_for_device.start_index,
            });
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
                    key.encoded_frost_key.clone(),
                    sign_item.clone(),
                    indexed_nonces,
                )
            })
            .collect();

        let key = key.clone();
        let sign_request = SignRequest {
            sign_task,
            nonces: signing_nonces.clone(),
            key_id,
        };

        self.action_state = Some(CoordinatorState::Signing {
            key: key.clone(),
            sign_state: SigningSessionState {
                targets: signing_parties.clone(),
                sessions,
                request: sign_request.clone(),
            },
        });

        let mut outgoing = vec![];

        outgoing.extend(used_nonces.into_iter().map(CoordinatorSend::ToStorage));
        outgoing.push(CoordinatorSend::ToDevice {
            destinations: signing_parties,
            message: CoordinatorToDeviceMessage::RequestSign(sign_request),
        });

        Ok(outgoing)
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
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SignSessionProgress {
    sign_item: SignItem,
    sign_session: SignSession,
    signature_shares: BTreeMap<DeviceId, Scalar<Public, Zero>>,
    root_key: EncodedFrostKey,
}

impl SignSessionProgress {
    pub fn new<NG>(
        frost: &Frost<sha2::Sha256, NG>,
        root_key: EncodedFrostKey,
        sign_item: SignItem,
        nonces: BTreeMap<frost::PartyIndex, frost::Nonce>,
    ) -> Self {
        let tweaked_key = sign_item.derive_key(&root_key.into_frost_key());
        let sign_session =
            frost.start_sign_session(&tweaked_key, nonces, sign_item.schnorr_fun_message());
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

    pub fn tweaked_frost_key(&self) -> FrostKey<EvenY> {
        self.sign_item.derive_key(&self.root_key.into_frost_key())
    }

    pub fn verify_final_signature<NG>(
        &self,
        schnorr: &Schnorr<sha2::Sha256, NG>,
        signature: &Signature,
    ) -> bool {
        self.sign_item.verify_final_signature(
            schnorr,
            self.root_key.into_frost_key().public_key(),
            signature,
        )
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
}

#[derive(Clone, Debug)]
pub enum KeyGenState {
    WaitingForResponses {
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        responses: BTreeMap<DeviceId, Option<KeyGenResponse>>,
        threshold: u16,
    },
    WaitingForAcks {
        frost_key: EncodedFrostKey,
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        acks: BTreeMap<DeviceId, bool>,
        session_hash: SessionHash,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize)]
pub struct CoordinatorFrostKey {
    encoded_frost_key: EncodedFrostKey,
    device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
}

impl CoordinatorFrostKey {
    pub fn threshold(&self) -> usize {
        self.encoded_frost_key.threshold()
    }

    pub fn frost_key(&self) -> FrostKey<Normal> {
        self.encoded_frost_key.clone().into()
    }

    pub fn key_id(&self) -> KeyId {
        self.encoded_frost_key.into_frost_key().key_id()
    }

    pub fn devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_to_share_index.keys().cloned()
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
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StartSignError {}
