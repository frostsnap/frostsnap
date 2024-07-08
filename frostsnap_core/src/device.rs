use crate::{
    gen_pop_message, message::*, ActionError, CheckedSignTask, Error, FrostKeyExt, MessageResult,
    NONCE_BATCH_SIZE,
};
use crate::{DeviceId, KeyId};
use alloc::collections::BTreeSet;
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use core::iter;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::frost::EncodedFrostKey;
use schnorr_fun::fun::poly;
use schnorr_fun::{
    binonce::{Nonce, NonceKeyPair},
    frost::{self},
    fun::{derive_nonce_rng, marker::*, KeyPair, Point, Scalar, Tag},
    nonce, Message,
};

use sha2::Sha256;

#[derive(Clone, Debug)]
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
            SignerState::DisplayBackup { .. } => TaskKind::DisplayBackup,
        };

        Some(DeviceSend::ToUser(DeviceToUserMessage::Canceled { task }))
    }

    pub fn keypair(&self) -> &KeyPair {
        &self.keypair
    }

    pub fn device_id(&self) -> DeviceId {
        DeviceId::new(self.keypair().public_key())
    }

    pub fn keys(&self) -> BTreeSet<KeyId> {
        self.keys.keys().cloned().collect()
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
        match (&self.action_state, message.clone()) {
            (_, RequestNonces) => {
                let nonces = self
                    .generate_nonces(self.nonce_counter)
                    .take(NONCE_BATCH_SIZE as usize)
                    .map(|nonce| nonce.public())
                    .collect();

                Ok(vec![DeviceSend::ToCoordinator(
                    DeviceToCoordinatorMessage::NonceResponse(DeviceNonces {
                        start_index: self.nonce_counter,
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
                let scalar_poly = poly::scalar::generate(threshold as usize, rng);

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
                CoordinatorToDeviceMessage::FinishKeyGen {
                    ref shares_provided,
                },
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
                    != &poly::scalar::to_point_poly(scalar_poly)
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
                let local_polys: BTreeMap<_, _> = iter::once((*my_index, scalar_poly)).collect();
                let keygen = frost
                    .new_keygen(point_polys, &local_polys)
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
                        encoded_frost_key: frost_key.into(),
                        secret_share,
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

                let checked_sign_task = sign_task
                    .check(key.key_id())
                    .map_err(|e| Error::signer_invalid_message(&message, e))?;

                let key_id = key.key_id();
                let my_nonces = nonces.get(&key.secret_share.index).ok_or_else(|| {
                    Error::signer_invalid_message(
                        &message,
                        "this device was asked to sign but no nonces
                were provided",
                    )
                })?;

                let n_signatures_requested = checked_sign_task.sign_items().len();
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
                    sign_task: checked_sign_task.clone(),
                    session_nonces: nonces,
                });
                Ok(vec![DeviceSend::ToUser(
                    DeviceToUserMessage::SignatureRequest {
                        sign_task: checked_sign_task,
                        key_id,
                    },
                )])
            }
            (None, CoordinatorToDeviceMessage::DisplayBackup { key_id }) => {
                let _key = self.keys.get(&key_id).ok_or(Error::signer_invalid_message(
                    &message,
                    "signer doesn't have a share for this key",
                ))?;

                self.action_state = Some(SignerState::DisplayBackup {
                    key_id,
                    awaiting_ack: true,
                });

                Ok(vec![DeviceSend::ToUser(
                    DeviceToUserMessage::DisplayBackupRequest { key_id },
                )])
            }
            _ => Err(Error::signer_message_kind(&self.action_state, &message)),
        }
    }

    pub fn keygen_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match self.action_state.take() {
            Some(SignerState::KeyGenAck { key }) => {
                let frost_key = key.encoded_frost_key.into_frost_key();
                let session_hash = frost_key.into_xonly_key().public_key().to_xonly_bytes();

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
                    .get(&key.secret_share.index)
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
                    let replenish_start = self.nonce_counter + my_session_nonces.nonces_remaining;
                    // How many nonces we should give them from that point
                    let replenish_amount =
                        NONCE_BATCH_SIZE.saturating_sub(my_session_nonces.nonces_remaining);

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
                let share_index = key.secret_share.index;
                let frost_key = key.encoded_frost_key.into_frost_key();
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

                    let derived_xonly_key = sign_item.app_tweak.derive_xonly_key(&frost_key);
                    let message = Message::raw(&sign_item.message[..]);
                    let sign_session =
                        frost.start_sign_session(&derived_xonly_key, nonces_at_index, message);

                    let sig_share = frost.sign(
                        &derived_xonly_key,
                        &sign_session,
                        secret_share,
                        secret_nonce,
                    );

                    assert!(frost.verify_signature_share(
                        &derived_xonly_key,
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

    pub fn display_backup_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match self.action_state {
            Some(SignerState::DisplayBackup {
                key_id,
                awaiting_ack: true,
            }) => {
                let key = self.keys.get(&key_id).expect("key must exist");
                let backup = key.secret_share.to_bech32_backup();

                self.action_state = Some(SignerState::DisplayBackup {
                    key_id,
                    awaiting_ack: false,
                });

                Ok(vec![
                    DeviceSend::ToCoordinator(DeviceToCoordinatorMessage::DisplayBackupConfirmed),
                    DeviceSend::ToUser(DeviceToUserMessage::DisplayBackup { key_id, backup }),
                ])
            }
            _ => Err(ActionError::WrongState {
                in_state: self.action_state_name(),
                action: "display_backup_ack",
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

#[derive(Clone, Debug)]
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
        sign_task: CheckedSignTask,
        session_nonces: BTreeMap<Scalar<Public, NonZero>, SignRequestNonces>,
    },
    DisplayBackup {
        key_id: KeyId,
        awaiting_ack: bool,
    },
}

impl SignerState {
    pub fn name(&self) -> &'static str {
        match self {
            SignerState::KeyGen { .. } => "KeyGen",
            SignerState::KeyGenAck { .. } => "KeyGenAck",
            SignerState::AwaitingSignAck { .. } => "AwaitingSignAck",
            SignerState::DisplayBackup { .. } => "DisplayBackup",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct FrostsnapSecretKey {
    /// The joint key
    pub encoded_frost_key: EncodedFrostKey,
    /// Our secret share of it
    pub secret_share: frost::SecretShare,
}

impl FrostsnapSecretKey {
    pub fn key_id(&self) -> KeyId {
        self.encoded_frost_key.into_frost_key().key_id()
    }

    pub fn public_key(&self) -> Point {
        self.encoded_frost_key.into_frost_key().public_key()
    }
}
