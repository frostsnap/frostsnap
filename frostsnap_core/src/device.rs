use crate::DeviceId;
use crate::{gen_pop_message, message::*, ActionError, Error, MessageResult, NONCE_BATCH_SIZE};
use alloc::{boxed::Box, collections::BTreeMap, string::ToString, vec::Vec};
use rand_chacha::ChaCha20Rng;
use schnorr_fun::{
    frost::{self, generate_scalar_poly, FrostKey},
    fun::{derive_nonce_rng, marker::*, KeyPair, Scalar, Tag},
    musig::{Nonce, NonceKeyPair},
    nonce, Message,
};
use sha2::Sha256;

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct FrostSigner {
    keypair: KeyPair,
    state: SignerState,
    nonce_counter: usize,
}

impl FrostSigner {
    pub fn new_random(rng: &mut impl rand_core::RngCore) -> Self {
        Self::new(KeyPair::<Normal>::new(Scalar::random(rng)))
    }

    pub fn new(keypair: KeyPair) -> Self {
        Self {
            keypair,
            state: SignerState::Registered,
            nonce_counter: 0,
        }
    }

    /// temporary hack until we store multiple keygens
    pub fn clear_state(&mut self) {
        *self = Self::new(self.keypair.clone())
    }

    #[must_use]
    pub fn cancel_action(&mut self) -> Vec<DeviceSend> {
        let message = match &self.state {
            SignerState::KeyGen { .. }
            | SignerState::FrostKey {
                awaiting_ack: true, ..
            } => {
                self.state = SignerState::Registered;
                Some(DeviceToUserMessage::Canceled {
                    task: TaskKind::KeyGen,
                })
            }
            SignerState::AwaitingSignAck { key, .. } => {
                self.state = SignerState::FrostKey {
                    key: key.clone(),
                    awaiting_ack: false,
                };
                Some(DeviceToUserMessage::Canceled {
                    task: TaskKind::Sign,
                })
            }
            SignerState::FrostKey { .. } | SignerState::Registered => None,
        };
        message.into_iter().map(DeviceSend::ToUser).collect()
    }

    pub fn keypair(&self) -> &KeyPair {
        &self.keypair
    }

    pub fn device_id(&self) -> DeviceId {
        DeviceId::new(self.keypair().public_key())
    }

    pub fn generate_nonces(
        &mut self,
        aux_rand: [u8; 32],
        start: usize,
        n: usize,
    ) -> impl Iterator<Item = NonceKeyPair> + '_ {
        let mut nonce_rng = derive_nonce_rng! {
            nonce_gen => nonce::Deterministic::<Sha256>::default().tag(b"frostsnap/nonces"),
            secret => self.keypair.secret_key(),
            public => [aux_rand],
            seedable_rng => ChaCha20Rng
        };

        nonce_rng.set_word_pos((start * 16) as u128);
        (0..n).map(move |_| NonceKeyPair::random(&mut nonce_rng))
    }

    pub fn state(&self) -> &SignerState {
        &self.state
    }

    pub fn recv_coordinator_message(
        &mut self,
        message: CoordinatorToDeviceMessage,
        rng: &mut impl rand_core::RngCore,
    ) -> MessageResult<Vec<DeviceSend>> {
        match (self.state.clone(), message.clone()) {
            (
                SignerState::Registered,
                CoordinatorToDeviceMessage::DoKeyGen {
                    device_to_share_index,
                    threshold,
                },
            ) => {
                if !device_to_share_index.contains_key(&self.device_id()) {
                    return Ok(vec![]);
                }
                let frost = frost::new_with_deterministic_nonces::<Sha256>();
                let mut aux_rand = [0u8; 32];
                rng.fill_bytes(&mut aux_rand);
                let scalar_poly = generate_scalar_poly(threshold, rng);

                let encrypted_shares = KeyGenProvideShares::generate(
                    &frost,
                    &scalar_poly,
                    &device_to_share_index,
                    rng,
                );

                self.state = SignerState::KeyGen {
                    scalar_poly,
                    device_to_share_index,
                    threshold,
                    aux_rand,
                };

                let nonces = self
                    .generate_nonces(aux_rand, 0, NONCE_BATCH_SIZE)
                    .map(|nonce| nonce.public())
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("correct length");

                Ok(vec![DeviceSend::ToCoordinator(
                    DeviceToCoordinatorMessage::KeyGenResponse(KeyGenResponse {
                        encrypted_shares,
                        nonces: Box::new(nonces),
                    }),
                )])
            }
            (
                SignerState::KeyGen {
                    device_to_share_index,
                    aux_rand,
                    scalar_poly,
                    ..
                },
                CoordinatorToDeviceMessage::FinishKeyGen { shares_provided },
            ) => {
                if let Some((device_id, _)) = device_to_share_index
                    .iter()
                    .find(|(device_id, _)| !shares_provided.contains_key(device_id))
                {
                    return Err(Error::signer_invalid_message(
                        &message,
                        format!("Missing shares from {}", device_id),
                    ));
                }
                let frost = frost::new_with_deterministic_nonces::<Sha256>();

                let point_polys: BTreeMap<_, _> = shares_provided
                    .iter()
                    .map(|(device_id, share)| {
                        (
                            *device_to_share_index
                                .get(device_id)
                                .expect("we checked we have shares"),
                            share.my_poly.clone(),
                        )
                    })
                    .collect();
                // Confirm our point poly matches what we expect
                let my_index = device_to_share_index
                    .get(&self.device_id())
                    .expect("we must exist");
                if point_polys
                    .get(my_index)
                    .expect("we have a point poly in this finish keygen")
                    != &frost::to_point_poly(&scalar_poly)
                {
                    return Err(Error::signer_invalid_message(
                        &message,
                        "Coordinator told us we are using a different point poly than we expected"
                            .to_string(),
                    ));
                }

                let transpose_shares = shares_provided
                    .keys()
                    .map(|device_id_receiver| {
                        Ok((
                            device_id_receiver,
                            shares_provided
                                .iter()
                                .map(|(provider_id, share)| {
                                    Ok((
                                        *provider_id,
                                        (
                                            share
                                                .encrypted_shares
                                                .get(device_id_receiver)
                                                .cloned()
                                                .ok_or(Error::signer_invalid_message(
                                                    &message,
                                                    format!(
                                                        "Missing shares for {}",
                                                        device_id_receiver
                                                    ),
                                                ))?,
                                            share.proof_of_possession.clone(),
                                        ),
                                    ))
                                })
                                .collect::<Result<BTreeMap<_, _>, _>>()?,
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;

                let my_shares = transpose_shares
                    .get(&self.device_id())
                    .expect("this device is part of the keygen")
                    .iter()
                    .map(|(provider_id, (encrypted_secret_share, pop))| {
                        (
                            *device_to_share_index
                                .get(provider_id)
                                .expect("just checked shares exist"),
                            (
                                encrypted_secret_share.decrypt(self.keypair().secret_key()),
                                pop.clone(),
                            ),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();

                let pop_message = gen_pop_message(device_to_share_index.keys().cloned());
                let keygen = frost
                    .new_keygen(point_polys)
                    .map_err(|e| Error::signer_message_error(&message, e))?;

                let (secret_share, frost_key) = frost
                    .finish_keygen(
                        keygen.clone(),
                        *my_index,
                        my_shares,
                        Message::raw(&pop_message),
                    )
                    .map_err(|e| Error::signer_message_error(&message, e))?;

                let session_hash = frost_key
                    .clone()
                    .into_xonly_key()
                    .public_key()
                    .to_xonly_bytes();

                self.state = SignerState::FrostKey {
                    key: FrostsnapKey {
                        frost_key,
                        secret_share,
                        share_index: *my_index,
                        aux_rand,
                    },
                    awaiting_ack: true,
                };

                Ok(vec![DeviceSend::ToUser(DeviceToUserMessage::CheckKeyGen {
                    session_hash,
                })])
            }
            (
                SignerState::FrostKey {
                    key,
                    awaiting_ack: false,
                },
                CoordinatorToDeviceMessage::RequestSign(SignRequest {
                    nonces, sign_task, ..
                }),
            ) => {
                let (my_nonces, my_nonce_index, _) = match nonces.get(&key.share_index) {
                    Some(nonce) => nonce,
                    None => return Ok(Vec::new()),
                };

                let n_signatures_requested = sign_task.sign_items().len();
                if my_nonces.len() != n_signatures_requested {
                    return Err(Error::signer_invalid_message(&message, format!( "Number of nonces ({}) was not the same as the number of signatures we were asked for {}", my_nonces.len(), n_signatures_requested)));
                }

                let expected_nonces = self
                    .generate_nonces(key.aux_rand, *my_nonce_index, my_nonces.len())
                    .map(|nonce| nonce.public())
                    .collect::<Vec<_>>();
                if expected_nonces != *my_nonces {
                    return Err(Error::signer_invalid_message(
                        &message,
                        "Signing request nonces do not match expected",
                    ));
                }

                if self.nonce_counter > *my_nonce_index {
                    return Err(Error::signer_invalid_message(
                        &message,
                        format!(
                            "Attempt to reuse nonces! Expected nonce >= {} but got {}",
                            self.nonce_counter, my_nonce_index
                        ),
                    ));
                }

                self.state = SignerState::AwaitingSignAck {
                    key: key.clone(),
                    sign_task: sign_task.clone(),
                    nonces,
                };
                Ok(vec![DeviceSend::ToUser(
                    DeviceToUserMessage::SignatureRequest { sign_task },
                )])
            }
            _ => Err(Error::signer_message_kind(&self.state, &message)),
        }
    }

    pub fn keygen_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match &mut self.state {
            SignerState::FrostKey {
                key, awaiting_ack, ..
            } if *awaiting_ack => {
                let session_hash = key
                    .frost_key
                    .clone()
                    .into_xonly_key()
                    .public_key()
                    .to_xonly_bytes();

                *awaiting_ack = false;
                Ok(vec![
                    DeviceSend::ToCoordinator(DeviceToCoordinatorMessage::KeyGenAck(session_hash)),
                    DeviceSend::ToStorage(DeviceToStorageMessage::SaveKey),
                ])
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state.name(),
                action: "keygen_ack",
            }),
        }
    }

    pub fn sign_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match self.state.clone() {
            SignerState::AwaitingSignAck {
                key,
                sign_task,
                nonces,
            } => {
                let sign_items = sign_task.sign_items();
                let frost = frost::new_with_deterministic_nonces::<Sha256>();
                let (_, my_nonce_index, my_replenish_index) =
                    nonces.get(&key.share_index).expect("already checked");

                // ⚠ Update nonce counter. Overflow would allow nonce reuse.
                self.nonce_counter = my_nonce_index.saturating_add(sign_items.len());

                let secret_nonces =
                    self.generate_nonces(key.aux_rand, *my_nonce_index, sign_items.len());

                let mut signature_shares = vec![];

                for (nonce_index, (sign_item, secret_nonce)) in
                    sign_items.iter().zip(secret_nonces).enumerate()
                {
                    let nonces_at_index = nonces
                        .iter()
                        .map(|(index, (nonces, _, _))| (*index, nonces[nonce_index]))
                        .collect();

                    let mut xpub = crate::xpub::Xpub::new(key.frost_key.clone());
                    xpub.derive_bip32(&sign_item.bip32_path);
                    let mut xonly_frost_key = xpub.key().clone().into_xonly_key();

                    if sign_item.tap_tweak {
                        let tweak = bitcoin::util::taproot::TapTweakHash::from_key_and_tweak(
                            bitcoin::XOnlyPublicKey::from_slice(
                                &xonly_frost_key.public_key().to_xonly_bytes(),
                            )
                            .unwrap(),
                            None,
                        )
                        .to_scalar();
                        xonly_frost_key = xonly_frost_key
                            .tweak(
                                Scalar::<Public, Zero>::from_slice(&tweak.to_be_bytes()).unwrap(),
                            )
                            .expect("computationally unreachable");
                    }

                    let message = Message::raw(&sign_item.message[..]);
                    let sign_session =
                        frost.start_sign_session(&xonly_frost_key, nonces_at_index, message);

                    let sig_share = frost.sign(
                        &xonly_frost_key,
                        &sign_session,
                        key.share_index,
                        &key.secret_share,
                        secret_nonce,
                    );

                    assert!(frost.verify_signature_share(
                        &xonly_frost_key,
                        &sign_session,
                        key.share_index,
                        sig_share,
                    ));

                    signature_shares.push(sig_share);
                }

                let replenish_nonces = self
                    .generate_nonces(key.aux_rand, *my_replenish_index, sign_items.len())
                    .map(|nonce| nonce.public())
                    .collect();

                self.state = SignerState::FrostKey {
                    key: key.clone(),
                    awaiting_ack: false,
                };

                Ok(vec![
                    DeviceSend::ToStorage(DeviceToStorageMessage::ExpendNonce),
                    DeviceSend::ToCoordinator(DeviceToCoordinatorMessage::SignatureShare {
                        signature_shares,
                        new_nonces: replenish_nonces,
                    }),
                ])
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state.name(),
                action: "sign_ack",
            }),
        }
    }

    pub fn frost_key(&self) -> Option<&FrostKey<Normal>> {
        match self.state() {
            SignerState::Registered => None,
            SignerState::KeyGen { .. } => None,
            SignerState::FrostKey { key, .. } => Some(&key.frost_key),
            SignerState::AwaitingSignAck { key, .. } => Some(&key.frost_key),
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum SignerState {
    Registered,
    KeyGen {
        scalar_poly: Vec<Scalar>,
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        threshold: usize,
        aux_rand: [u8; 32],
    },
    AwaitingSignAck {
        key: FrostsnapKey,
        sign_task: SignTask,
        nonces: BTreeMap<Scalar<Public, NonZero>, (Vec<Nonce>, usize, usize)>,
    },
    FrostKey {
        key: FrostsnapKey,
        awaiting_ack: bool,
    },
}

impl SignerState {
    pub fn name(&self) -> &'static str {
        match self {
            SignerState::Registered => "Registered",
            SignerState::KeyGen { .. } => "KeyGen",
            SignerState::AwaitingSignAck { .. } => "AwaitingSignAck",
            SignerState::FrostKey { .. } => "FrostKey",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct FrostsnapKey {
    /// The joint key
    pub frost_key: FrostKey<Normal>,
    /// Our secret share of it
    pub secret_share: Scalar,
    /// Our secret share index
    pub share_index: Scalar<Public, NonZero>,
    /// auxilliary randomness for generating nonces
    pub aux_rand: [u8; 32],
}

#[derive(Debug, Clone)]
pub enum StartSignError {
    UnknownDevice {
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
            StartSignError::UnknownDevice { device_id } => {
                write!(f, "Unknown device {}", device_id)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StartSignError {}
