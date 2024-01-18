use crate::{
    gen_pop_message, message::*, ActionError, Error, FrostKeyExt, MessageResult, SessionHash,
    StartSignError,
};
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    vec::Vec,
};
use schnorr_fun::{
    frost::{self, FrostKey, SignSession},
    fun::{marker::*, Scalar},
    musig::Nonce,
    Message,
};
use sha2::Sha256;

use crate::DeviceId;

impl Default for CoordinatorState {
    fn default() -> Self {
        Self::Registration
    }
}

impl CoordinatorState {
    pub fn name(&self) -> &'static str {
        match self {
            CoordinatorState::Registration => "Registration",
            CoordinatorState::KeyGen(keygen_state) => match keygen_state {
                KeyGenState::WaitingForResponses { .. } => "WaitingForResponses",
                KeyGenState::WaitingForAcks { .. } => "WaitingForAcks",
            },
            CoordinatorState::FrostKey { .. } => "FrostKey",
            CoordinatorState::Signing { .. } => "Signing",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SignSessionProgress {
    sign_session: SignSession,
    signature_shares: BTreeMap<DeviceId, Scalar<Public, Zero>>,
    key: FrostKey<EvenY>,
}

impl SignSessionProgress {
    pub fn received_from(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.signature_shares.keys().cloned()
    }
}

#[derive(Clone, Debug)]
pub enum CoordinatorState {
    Registration,
    KeyGen(KeyGenState),
    FrostKey {
        key: CoordinatorFrostKeyState,
    },
    Signing {
        sign_state: SigningSessionState,
        key: CoordinatorFrostKeyState,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SigningSessionState {
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
        responses: BTreeMap<DeviceId, Option<KeyGenResponse>>,
        threshold: usize,
    },
    WaitingForAcks {
        frost_key: FrostKey<Normal>,
        device_nonces: BTreeMap<DeviceId, DeviceNonces>,
        acks: BTreeMap<DeviceId, bool>,
        session_hash: SessionHash,
    },
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize)]
pub struct DeviceNonces {
    counter: usize,
    nonces: VecDeque<Nonce>,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize)]
pub struct CoordinatorFrostKeyState {
    frost_key: FrostKey<Normal>,
    device_nonces: BTreeMap<DeviceId, DeviceNonces>,
}

impl CoordinatorFrostKeyState {
    pub fn devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_nonces.keys().cloned()
    }

    pub fn threshold(&self) -> usize {
        self.frost_key.threshold()
    }

    pub fn frost_key(&self) -> &FrostKey<Normal> {
        &self.frost_key
    }

    pub fn nonces_left(&self, device_id: DeviceId) -> Option<usize> {
        let device_nonces = self.device_nonces.get(&device_id)?;
        Some(device_nonces.nonces.len())
    }
}

impl FrostCoordinator {
    pub fn new() -> Self {
        Self {
            state: CoordinatorState::Registration,
        }
    }

    pub fn from_stored_key(key: CoordinatorFrostKeyState) -> Self {
        Self {
            state: CoordinatorState::FrostKey { key },
        }
    }

    pub fn restore_sign_session(&mut self, sign_state: SigningSessionState) {
        let key = self
            .frost_key_state()
            .expect("cannot restore in coordinator without state")
            .clone();
        self.state = CoordinatorState::Signing { sign_state, key };
    }

    pub fn recv_device_message(
        &mut self,
        from: DeviceId,
        message: DeviceToCoordinatorMessage,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        match &mut self.state {
            CoordinatorState::Registration => {
                Err(Error::coordinator_message_kind(&self.state, &message))
            }
            CoordinatorState::KeyGen(keygen_state) => match keygen_state {
                KeyGenState::WaitingForResponses {
                    responses,
                    threshold,
                } => {
                    match &message {
                        DeviceToCoordinatorMessage::KeyGenResponse(new_shares) => {
                            if let Some(existing) = responses.insert(from, Some(new_shares.clone()))
                            {
                                debug_assert!(
                                    existing.is_none(),
                                    "Device sent keygen response twice"
                                );
                            }

                            if new_shares.encrypted_shares.my_poly.len() != *threshold {
                                return Err(Error::coordinator_invalid_message(
                                    &message,
                                    "Device sent polynomial with incorrect threshold",
                                ));
                            }

                            let mut outgoing =
                                vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
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
                                                device_id.to_poly_index(),
                                                response.encrypted_shares.my_poly.clone(),
                                            )
                                        })
                                        .collect();
                                    let proofs_of_possession = responses
                                        .iter()
                                        .map(|(device_id, response)| {
                                            (
                                                device_id.to_poly_index(),
                                                response
                                                    .encrypted_shares
                                                    .proof_of_possession
                                                    .clone(),
                                            )
                                        })
                                        .collect();
                                    let frost = frost::new_without_nonce_generation::<Sha256>();
                                    let keygen = frost.new_keygen(point_polys).unwrap();
                                    // let keygen_id = frost.keygen_id(&keygen);
                                    let pop_message = gen_pop_message(responses.keys().cloned());

                                    let frost_key = match frost.finish_keygen_coordinator(keygen, proofs_of_possession, Message::raw(&pop_message)) {
                                        Ok(frost_key) => frost_key,
                                        Err(_) => todo!("should notify user somehow that everything was fucked and we're canceling it"),
                                    };

                                    let device_nonces = responses
                                        .iter()
                                        .map(|(device_id, response)| {
                                            let device_nonces = DeviceNonces {
                                                counter: 0,
                                                nonces: response.nonces.iter().cloned().collect(),
                                            };
                                            (*device_id, device_nonces)
                                        })
                                        .collect();

                                    // TODO: This is definitely insufficient
                                    let session_hash = frost_key
                                        .clone()
                                        .into_xonly_key()
                                        .public_key()
                                        .to_xonly_bytes();

                                    self.state =
                                        CoordinatorState::KeyGen(KeyGenState::WaitingForAcks {
                                            frost_key,
                                            device_nonces,
                                            acks: responses
                                                .clone()
                                                .into_keys()
                                                .map(|id| (id, false))
                                                .collect(),
                                            session_hash,
                                        });

                                    // TODO: check order
                                    outgoing.push(CoordinatorSend::ToDevice(
                                        CoordinatorToDeviceMessage::FinishKeyGen {
                                            shares_provided: responses
                                                .into_iter()
                                                .map(|(id, response)| {
                                                    (id, response.encrypted_shares)
                                                })
                                                .collect(),
                                        },
                                    ));
                                    outgoing.push(CoordinatorSend::ToUser(
                                        CoordinatorToUserMessage::KeyGen(
                                            CoordinatorToUserKeyGenMessage::CheckKeyGen {
                                                session_hash,
                                            },
                                        ),
                                    ));
                                }
                                None => { /* not finished yet  */ }
                            };
                            Ok(outgoing)
                        }

                        _ => Err(Error::coordinator_message_kind(&self.state, &message)),
                    }
                }
                KeyGenState::WaitingForAcks {
                    frost_key,
                    device_nonces,
                    acks,
                    session_hash,
                } => match message {
                    DeviceToCoordinatorMessage::KeyGenAck(acked_session_hash) => {
                        let mut outgoing = vec![];
                        if acked_session_hash != *session_hash {
                            return Err(Error::coordinator_invalid_message(
                                &message,
                                "Device acked wrong keygen session hash",
                            ));
                        }

                        match acks.get_mut(&from) {
                            None => {
                                return Err(Error::coordinator_invalid_message(
                                    &message,
                                    "Received ack from device not a member of keygen",
                                ));
                            }
                            Some(ack) => {
                                outgoing.push(CoordinatorSend::ToUser(
                                    CoordinatorToUserMessage::KeyGen(
                                        CoordinatorToUserKeyGenMessage::KeyGenAck { from },
                                    ),
                                ));
                                *ack = true;
                            }
                        }

                        let all_acks = acks.values().all(|ack| *ack);
                        if all_acks {
                            let key = CoordinatorFrostKeyState {
                                frost_key: frost_key.clone(),
                                device_nonces: device_nonces.clone(),
                            };
                            let key_id = key.frost_key.key_id();
                            self.state = CoordinatorState::FrostKey { key: key.clone() };
                            outgoing.extend([
                                CoordinatorSend::ToStorage(
                                    CoordinatorToStorageMessage::UpdateFrostKey(key),
                                ),
                                CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen(
                                    CoordinatorToUserKeyGenMessage::FinishedKey { key_id },
                                )),
                            ]);
                        }
                        Ok(outgoing)
                    }
                    _ => Err(Error::coordinator_message_kind(&self.state, &message)),
                },
            },
            CoordinatorState::Signing { key, sign_state } => match &message {
                DeviceToCoordinatorMessage::SignatureShare {
                    signature_shares,
                    new_nonces,
                } => {
                    let sessions = &mut sign_state.sessions;
                    let n_signatures = sessions.len();
                    let frost = frost::new_without_nonce_generation::<Sha256>();
                    let mut outgoing = vec![];

                    let nonce_for_device = key.device_nonces.get_mut(&from).ok_or(
                        Error::coordinator_invalid_message(&message, "Signer is unknown"),
                    )?;

                    if new_nonces.len() != n_signatures {
                        return Err(Error::coordinator_invalid_message(
                            &message,
                            format!(
                                "Signer did not replenish the correct number of nonces: {}",
                                n_signatures
                            ),
                        ));
                    }

                    if signature_shares.len() != n_signatures {
                        return Err(Error::coordinator_invalid_message(&message, format!("signer did not provide the right number of signature shares. Got {}, expected {}", signature_shares.len(), sessions.len())));
                    }

                    // first we do a validation loop and then another loop to actually insert the
                    // signature shares so that we don't mutate self unless the entire message is
                    // valid.
                    for (session_progress, signature_share) in sessions.iter().zip(signature_shares)
                    {
                        let session = &session_progress.sign_session;
                        let xonly_frost_key = &session_progress.key;
                        if !session
                            .participants()
                            .any(|x_coord| x_coord == from.to_poly_index())
                        {
                            return Err(Error::coordinator_invalid_message(
                                &message,
                                "Signer was not a particpant for this session",
                            ));
                        }

                        if !frost.verify_signature_share(
                            xonly_frost_key,
                            session,
                            from.to_poly_index(),
                            *signature_share,
                        ) {
                            return Err(Error::coordinator_invalid_message(
                                &message,
                                format!(
                                    "Inavlid signature share under key {}",
                                    xonly_frost_key.public_key()
                                ),
                            ));
                        }
                    }

                    outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::Signing(
                        CoordinatorToUserSigningMessage::GotShare { from },
                    )));

                    for (session_progress, signature_share) in
                        sessions.iter_mut().zip(signature_shares)
                    {
                        session_progress
                            .signature_shares
                            .insert(from, *signature_share);
                    }

                    nonce_for_device.nonces.extend(new_nonces.iter());
                    // update state to save new nonces
                    outgoing.push(CoordinatorSend::ToStorage(
                        CoordinatorToStorageMessage::UpdateFrostKey(key.clone()),
                    ));

                    // the coordinator may want to persist this so a signing session can be restored

                    let all_finished = sessions
                        .iter()
                        .all(|session| session.signature_shares.len() == key.frost_key.threshold());

                    outgoing.push(CoordinatorSend::ToStorage(
                        CoordinatorToStorageMessage::StoreSigningState(sign_state.clone()),
                    ));

                    if all_finished {
                        let sessions = &sign_state.sessions;

                        let signatures = sessions
                            .iter()
                            .map(|session_progress| {
                                frost.combine_signature_shares(
                                    &session_progress.key,
                                    &session_progress.sign_session,
                                    session_progress
                                        .signature_shares
                                        .iter()
                                        .map(|(_, &share)| share)
                                        .collect(),
                                )
                            })
                            .map(EncodedSignature::new)
                            .collect();

                        self.state = CoordinatorState::FrostKey { key: key.clone() };

                        outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::Signing(
                            CoordinatorToUserSigningMessage::Signed { signatures },
                        )));
                    }

                    Ok(outgoing)
                }
                _ => Err(Error::coordinator_message_kind(&self.state, &message)),
            },
            _ => Err(Error::coordinator_message_kind(&self.state, &message)),
        }
    }

    pub fn do_keygen(
        &mut self,
        devices: &BTreeSet<DeviceId>,
        threshold: usize,
    ) -> Result<CoordinatorToDeviceMessage, ActionError> {
        if devices.len() < threshold {
            panic!(
                "caller needs to ensure that threshold < devices.len(). Tried {}-of-{}",
                threshold,
                devices.len()
            );
        }
        match self.state {
            CoordinatorState::Registration => {
                self.state = CoordinatorState::KeyGen(KeyGenState::WaitingForResponses {
                    responses: devices.iter().map(|&device_id| (device_id, None)).collect(),
                    threshold,
                });

                Ok(CoordinatorToDeviceMessage::DoKeyGen {
                    devices: devices.clone(),
                    threshold,
                })
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state.name(),
                action: "do_keygen",
            }),
        }
    }

    pub fn start_sign(
        &mut self,
        sign_task: SignTask,
        signing_parties: BTreeSet<DeviceId>,
    ) -> Result<Vec<CoordinatorSend>, StartSignError> {
        match &mut self.state {
            CoordinatorState::FrostKey { key } => {
                let selected = signing_parties.len();
                if selected < key.frost_key.threshold() {
                    return Err(StartSignError::NotEnoughDevicesSelected {
                        selected,
                        threshold: key.frost_key.threshold(),
                    });
                }

                let sign_items = sign_task.sign_items();
                let n_signatures = sign_items.len();

                let signing_nonces = signing_parties
                    .into_iter()
                    .map(|device_id| {
                        let nonces_for_device = key
                            .device_nonces
                            .get_mut(&device_id)
                            .ok_or(StartSignError::UnknownDevice { device_id })?;
                        let index_of_first_nonce = nonces_for_device.counter;
                        let index_of_last_nonce =
                            index_of_first_nonce + nonces_for_device.nonces.len();
                        let nonces = nonces_for_device
                            .nonces
                            .iter()
                            .take(n_signatures)
                            .cloned()
                            .collect::<Vec<_>>();
                        if nonces.len() < n_signatures {
                            return Err(StartSignError::NotEnoughNoncesForDevice {
                                device_id,
                                have: nonces.len(),
                                need: n_signatures,
                            });
                        }
                        let mut remaining = nonces_for_device.nonces.split_off(n_signatures);
                        core::mem::swap(&mut nonces_for_device.nonces, &mut remaining);
                        nonces_for_device.counter += n_signatures;

                        Ok((
                            device_id,
                            (nonces, index_of_first_nonce, index_of_last_nonce),
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;

                let frost = frost::new_without_nonce_generation::<Sha256>();

                let sessions = sign_items
                    .iter()
                    .enumerate()
                    .map(|(i, sign_item)| {
                        let b_message = Message::raw(&sign_item.message[..]);
                        let indexed_nonces = signing_nonces
                            .iter()
                            .map(|(id, (nonce, _, _))| (id.to_poly_index(), nonce[i]))
                            .collect();

                        let xonly_frost_key = sign_item.derive_key(key.frost_key());

                        let sign_session =
                            frost.start_sign_session(&xonly_frost_key, indexed_nonces, b_message);
                        SignSessionProgress {
                            sign_session,
                            key: xonly_frost_key,
                            signature_shares: Default::default(),
                        }
                    })
                    .collect();

                let key = key.clone();
                let sign_request = SignRequest {
                    sign_task,
                    nonces: signing_nonces.clone(),
                };
                self.state = CoordinatorState::Signing {
                    key: key.clone(),
                    sign_state: SigningSessionState {
                        sessions,
                        request: sign_request.clone(),
                    },
                };
                Ok(vec![
                    CoordinatorSend::ToStorage(CoordinatorToStorageMessage::UpdateFrostKey(key)),
                    CoordinatorSend::ToDevice(CoordinatorToDeviceMessage::RequestSign(
                        sign_request,
                    )),
                ])
            }
            _ => Err(StartSignError::CantSignInState {
                in_state: self.state().name(),
            }),
        }
    }

    pub fn state(&self) -> &CoordinatorState {
        &self.state
    }

    pub fn frost_key_state(&self) -> Option<&CoordinatorFrostKeyState> {
        match self.state() {
            CoordinatorState::FrostKey { key } => Some(key),
            _ => None,
        }
    }

    pub fn cancel(&mut self) {
        let state = core::mem::replace(&mut self.state, CoordinatorState::Registration);
        self.state = match state {
            CoordinatorState::KeyGen(_) => CoordinatorState::Registration,
            CoordinatorState::Signing { key, .. } => CoordinatorState::FrostKey { key },
            _ => state,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FrostCoordinator {
    state: CoordinatorState,
}
