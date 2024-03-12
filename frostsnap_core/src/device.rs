use crate::{
    gen_pop_message, message::*, ActionError, Error, FrostKeyExt, MessageResult, NONCE_BATCH_SIZE,
};
use crate::{DeviceId, KeyId};
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use rand_chacha::ChaCha20Rng;
use schnorr_fun::{
    binonce::{Nonce, NonceKeyPair},
    frost::{self, generate_scalar_poly, FrostKey},
    fun::{derive_nonce_rng, marker::*, KeyPair, Scalar, Tag},
    nonce, Message,
};
use sha2::Sha256;

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct FrostSigner {
    keypair: KeyPair,
    keys: BTreeMap<KeyId, FrostsnapSecretKey>,
    action_state: Option<SignerState>,
    nonce_counter: u64,
}

impl FrostSigner {
    pub fn new_random(rng: &mut impl rand_core::RngCore) -> Self {
        Self::new(KeyPair::<Normal>::new(Scalar::random(rng)))
    }

    pub fn new(keypair: KeyPair) -> Self {
        Self {
            keypair,
            keys: Default::default(),
            action_state: None,
            nonce_counter: 0,
        }
    }

    pub fn apply_change(&mut self, change: DeviceToStorageMessage) {
        match change {
            DeviceToStorageMessage::SaveKey(key) => {
                self.keys.insert(key.key_id(), key);
            }
            DeviceToStorageMessage::ExpendNonce { nonce_counter } => {
                self.nonce_counter = self.nonce_counter.max(nonce_counter);
            }
        }
    }

    #[must_use]
    pub fn cancel_action(&mut self) -> Option<DeviceSend> {
        let task = match self.action_state.take()? {
            SignerState::KeyGen { .. } | SignerState::KeyGenAck { .. } => TaskKind::KeyGen,
            SignerState::AwaitingSignAck { .. } => TaskKind::Sign,
        };

        Some(DeviceSend::ToUser(DeviceToUserMessage::Canceled { task }))
    }

    pub fn keypair(&self) -> &KeyPair {
        &self.keypair
    }

    pub fn device_id(&self) -> DeviceId {
        DeviceId::new(self.keypair().public_key())
    }

    fn generate_nonces(
        &self,
        // this is always the device key for now but because of lifetimes issues it has to be passed in
        start: u64,
    ) -> impl Iterator<Item = NonceKeyPair> + '_ {
        let mut nonce_rng = derive_nonce_rng! {
            nonce_gen => nonce::Deterministic::<Sha256>::default().tag(b"frostsnap/nonces"),
            secret => self.keypair.secret_key(),
            public => [b""],
            seedable_rng => ChaCha20Rng
        };

        nonce_rng.set_word_pos((start * 16) as u128);
        core::iter::from_fn(move || Some(NonceKeyPair::random(&mut nonce_rng)))
    }

    pub fn generate_public_nonces(&self, start: u64) -> impl Iterator<Item = Nonce> + '_ {
        self.generate_nonces(start).map(|nonce| nonce.public())
    }

    pub fn recv_coordinator_message(
        &mut self,
        message: CoordinatorToDeviceMessage,
        rng: &mut impl rand_core::RngCore,
    ) -> MessageResult<Vec<DeviceSend>> {
        use CoordinatorToDeviceMessage::*;
        match (self.action_state.clone(), message.clone()) {
            (_, RequestNonces(requested_nonces)) => {
                let nonces = self
                    .generate_nonces(
                        self.nonce_counter + NONCE_BATCH_SIZE.saturating_sub(requested_nonces),
                    )
                    .take(requested_nonces as usize)
                    .map(|nonce| nonce.public())
                    .collect();

                Ok(vec![DeviceSend::ToCoordinator(
                    DeviceToCoordinatorMessage::NonceResponse(DeviceNonces {
                        start_index: self.nonce_counter
                            + NONCE_BATCH_SIZE.saturating_sub(requested_nonces),
                        nonces,
                    }),
                )])
            }
            (
                None,
                DoKeyGen {
                    device_to_share_index,
                    threshold,
                },
            ) => {
                if !device_to_share_index.contains_key(&self.device_id()) {
                    return Ok(vec![]);
                }
                let frost = frost::new_with_deterministic_nonces::<Sha256>();
                let scalar_poly = generate_scalar_poly(threshold as usize, rng);

                let encrypted_shares =
                    KeyGenResponse::generate(&frost, &scalar_poly, &device_to_share_index, rng);

                self.action_state = Some(SignerState::KeyGen {
                    scalar_poly,
                    device_to_share_index,
                    threshold,
                });

                Ok(vec![DeviceSend::ToCoordinator(
                    DeviceToCoordinatorMessage::KeyGenResponse(encrypted_shares),
                )])
            }
            (
                Some(SignerState::KeyGen {
                    device_to_share_index,
                    scalar_poly,
                    ..
                }),
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

                self.action_state = Some(SignerState::KeyGenAck {
                    key: FrostsnapSecretKey {
                        frost_key,
                        secret_share,
                        share_index: *my_index,
                    },
                });

                Ok(vec![DeviceSend::ToUser(DeviceToUserMessage::CheckKeyGen {
                    session_hash,
                })])
            }
            (
                None,
                CoordinatorToDeviceMessage::RequestSign(SignRequest {
                    nonces,
                    sign_task,
                    key_id,
                }),
            ) => {
                let key = self.keys.get(&key_id).ok_or_else(|| {
                    Error::signer_invalid_message(
                        &message,
                        // we could instead send back a message saying we don't have this key but I
                        // think this will never happen in practice unless we have a way for one
                        // coordinator to delete a key from a device without the other coordinator
                        // knowing.
                        format!("device doesn't have key for {key_id}"),
                    )
                })?;

                let key_id = key.key_id();
                let my_nonces = nonces.get(&key.share_index).ok_or_else(|| {
                    Error::signer_invalid_message(
                        &message,
                        "this device was asked to sign but no nonces
                were provided",
                    )
                })?;

                let n_signatures_requested = sign_task.sign_items().len();
                if my_nonces.nonces.len() != n_signatures_requested {
                    return Err(Error::signer_invalid_message(&message, format!("Number of nonces ({}) was not the same as the number of signatures we were asked for {}", my_nonces.nonces.len(), n_signatures_requested)));
                }

                let expected_nonces = self
                    .generate_nonces(my_nonces.start)
                    .take(my_nonces.nonces.len())
                    .map(|nonce| nonce.public())
                    .collect::<Vec<_>>();
                if expected_nonces != my_nonces.nonces {
                    return Err(Error::signer_invalid_message(
                        &message,
                        "Signing request nonces do not match expected",
                    ));
                }

                if self.nonce_counter > my_nonces.start {
                    return Err(Error::signer_invalid_message(
                        &message,
                        format!(
                            "Attempt to reuse nonces! Expected nonce >= {} but got {}",
                            self.nonce_counter, my_nonces.start
                        ),
                    ));
                }

                self.action_state = Some(SignerState::AwaitingSignAck {
                    key_id,
                    sign_task: sign_task.clone(),
                    session_nonces: nonces,
                });
                Ok(vec![DeviceSend::ToUser(
                    DeviceToUserMessage::SignatureRequest { sign_task, key_id },
                )])
            }
            _ => Err(Error::signer_message_kind(&self.action_state, &message)),
        }
    }

    pub fn keygen_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match self.action_state.take() {
            Some(SignerState::KeyGenAck { key }) => {
                let session_hash = key
                    .frost_key
                    .clone()
                    .into_xonly_key()
                    .public_key()
                    .to_xonly_bytes();

                self.keys.insert(key.key_id(), key.clone());
                Ok(vec![
                    DeviceSend::ToCoordinator(DeviceToCoordinatorMessage::KeyGenAck(session_hash)),
                    DeviceSend::ToStorage(DeviceToStorageMessage::SaveKey(key.clone())),
                ])
            }
            action_state => {
                self.action_state = action_state;
                Err(ActionError::WrongState {
                    in_state: self.action_state_name(),
                    action: "keygen_ack",
                })
            }
        }
    }

    pub fn sign_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match &self.action_state {
            Some(SignerState::AwaitingSignAck {
                key_id,
                sign_task,
                session_nonces,
            }) => {
                let key = self
                    .keys
                    .get(key_id)
                    .ok_or(ActionError::StateInconsistent(format!(
                        "key {key_id} no longer exists so can't sign"
                    )))?;
                let secret_share = &key.secret_share;
                let my_session_nonces = session_nonces
                    .get(&key.share_index)
                    .expect("already checked");

                let sign_items = sign_task.sign_items();

                let new_nonces = {
                    // ⚠ Update nonce counter. Overflow would allow nonce reuse.
                    //
                    // hacktuallly this doesn't prevent nonce reuse. You can still re-use the nonce at
                    // u64::MAX.
                    //
                    self.nonce_counter = my_session_nonces
                        .start
                        .saturating_add(sign_items.len() as u64);

                    // This calculates the index after the last nonce the coordinator had. This is
                    // where we want to start providing new nonces.
                    let replenish_start = self.nonce_counter + NONCE_BATCH_SIZE;
                    // How many nonces we should give them from that point
                    let replenish_amount = sign_items.len();

                    let replenish_nonces = self
                        .generate_nonces(replenish_start)
                        .take(replenish_amount as usize)
                        .map(|nonce| nonce.public())
                        .collect();

                    DeviceNonces {
                        start_index: replenish_start,
                        nonces: replenish_nonces,
                    }
                };

                let secret_nonces = self
                    .generate_nonces(my_session_nonces.start)
                    .take(my_session_nonces.nonces.len());

                let frost = frost::new_without_nonce_generation::<Sha256>();
                let share_index = key.share_index;
                let mut xpub = crate::xpub::Xpub::new(key.frost_key.clone());

                let mut signature_shares = vec![];

                for (signature_index, (sign_item, secret_nonce)) in
                    sign_items.iter().zip(secret_nonces).enumerate()
                {
                    let nonces_at_index = session_nonces
                        .iter()
                        .map(|(signer_index, sign_req_nonces)| {
                            (*signer_index, sign_req_nonces.nonces[signature_index])
                        })
                        .collect();

                    xpub.derive_bip32(&sign_item.bip32_path);
                    let mut xonly_frost_key = xpub.key().clone().into_xonly_key();

                    if sign_item.tap_tweak {
                        let tweak = bitcoin::taproot::TapTweakHash::from_key_and_tweak(
                            bitcoin::key::XOnlyPublicKey::from_slice(
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
                        share_index,
                        secret_share,
                        secret_nonce,
                    );

                    assert!(frost.verify_signature_share(
                        &xonly_frost_key,
                        &sign_session,
                        share_index,
                        sig_share,
                    ));

                    signature_shares.push(sig_share);
                }

                self.action_state = None;

                Ok(vec![
                    DeviceSend::ToStorage(DeviceToStorageMessage::ExpendNonce {
                        nonce_counter: self.nonce_counter,
                    }),
                    DeviceSend::ToCoordinator(DeviceToCoordinatorMessage::SignatureShare {
                        signature_shares,
                        new_nonces,
                    }),
                ])
            }
            _ => Err(ActionError::WrongState {
                in_state: self.action_state_name(),
                action: "sign_ack",
            }),
        }
    }

    pub fn action_state_name(&self) -> &'static str {
        self.action_state
            .as_ref()
            .map(|x| x.name())
            .unwrap_or("None")
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum SignerState {
    KeyGen {
        scalar_poly: Vec<Scalar>,
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        threshold: u16,
    },
    KeyGenAck {
        key: FrostsnapSecretKey,
    },
    AwaitingSignAck {
        key_id: KeyId,
        sign_task: SignTask,
        session_nonces: BTreeMap<Scalar<Public, NonZero>, SignRequestNonces>,
    },
}

impl SignerState {
    pub fn name(&self) -> &'static str {
        match self {
            SignerState::KeyGen { .. } => "KeyGen",
            SignerState::KeyGenAck { .. } => "KeyGenAck",
            SignerState::AwaitingSignAck { .. } => "AwaitingSignAck",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct FrostsnapSecretKey {
    /// The joint key
    pub frost_key: FrostKey<Normal>,
    /// Our secret share of it
    pub secret_share: Scalar,
    /// Our secret share index
    pub share_index: Scalar<Public, NonZero>,
}

impl FrostsnapSecretKey {
    pub fn key_id(&self) -> KeyId {
        self.frost_key.key_id()
    }
}
