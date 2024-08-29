use core::num::NonZeroU32;

use crate::{
    message::*, ActionError, CheckedSignTask, Error, FrostKeyExt, MessageResult, SessionHash,
    NONCE_BATCH_SIZE,
};
use crate::{DeviceId, KeyId};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use rand_chacha::ChaCha20Rng;
use schnorr_fun::binonce;
use schnorr_fun::frost::chilldkg::encpedpop;
use schnorr_fun::frost::{PairedSecretShare, PartyIndex};
use schnorr_fun::fun::KeyPair;
use schnorr_fun::{
    binonce::{Nonce, NonceKeyPair},
    frost::{self},
    fun::{derive_nonce_rng, prelude::*, Tag},
    nonce, Message,
};
use sha2::digest::{FixedOutput, Update};

use sha2::Sha256;

#[derive(Clone, Debug)]
pub struct FrostSigner {
    keypair: KeyPair,
    secret_shares: BTreeMap<KeyId, PairedSecretShare>,
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
            secret_shares: Default::default(),
            action_state: None,
            nonce_counter: 0,
        }
    }

    pub fn apply_change(&mut self, change: DeviceToStorageMessage) {
        match change {
            DeviceToStorageMessage::SaveKey(key) => {
                self.secret_shares.insert(key.key_id(), key);
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
        self.secret_shares.keys().cloned().collect()
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
                    key_name,
                },
            ) => {
                if !device_to_share_index.contains_key(&self.device_id()) {
                    return Ok(vec![]);
                }
                let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();

                let share_receivers_enckeys = device_to_share_index
                    .iter()
                    .map(|(device, share_index)| (PartyIndex::from(*share_index), device.pubkey()))
                    .collect::<BTreeMap<_, _>>();
                let my_index = device_to_share_index
                    .get(&self.device_id())
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            format!(
                                "my device id {} was not party of the keygen",
                                self.device_id()
                            ),
                        )
                    })?;

                let (input_state, keygen_input) = encpedpop::Contributor::gen_keygen_input(
                    &schnorr,
                    threshold as u32,
                    &share_receivers_enckeys,
                    (*my_index).into(),
                    rng,
                );

                self.action_state = Some(SignerState::KeyGen {
                    input_state,
                    device_to_share_index,
                    threshold,
                    key_name,
                });

                Ok(vec![DeviceSend::ToCoordinator(
                    DeviceToCoordinatorMessage::KeyGenResponse(keygen_input),
                )])
            }
            (
                Some(SignerState::KeyGen {
                    device_to_share_index,
                    input_state,
                    key_name,
                    ..
                }),
                CoordinatorToDeviceMessage::FinishKeyGen { agg_input },
            ) => {
                let key_name = key_name.clone();
                input_state
                    .clone()
                    .verify_agg_input(&agg_input)
                    .map_err(|e| Error::signer_invalid_message(&message, e))?;

                let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
                // Note we could just store my_index in our state. But we want to keep aroudn the
                // keys of other devices for when we move to a certification based keygen.
                let my_index = device_to_share_index
                    .get(&self.device_id())
                    .expect("already checked");

                let secret_share = encpedpop::receive_share(
                    &schnorr,
                    (*my_index).into(),
                    &self.keypair,
                    &agg_input,
                )
                .map_err(|e| Error::signer_invalid_message(&message, e))
                .and_then(|secret_share| {
                    secret_share.non_zero().ok_or(Error::signer_invalid_message(
                        &message,
                        "keygen produced a zero shared key",
                    ))
                })?;

                let session_hash = Sha256::default()
                    .chain(agg_input.cert_bytes())
                    .finalize_fixed()
                    .into();

                self.action_state = Some(SignerState::KeyGenAck {
                    secret_share,
                    session_hash,
                });

                Ok(vec![DeviceSend::ToUser(DeviceToUserMessage::CheckKeyGen {
                    key_id: secret_share.public_key().key_id(),
                    session_hash,
                    key_name,
                })])
            }
            (None, CoordinatorToDeviceMessage::RequestSign(sign_req)) => {
                let key_id = sign_req.key_id;
                let nonces = sign_req.nonces.clone();
                let key = self.secret_shares.get(&key_id).ok_or_else(|| {
                    Error::signer_invalid_message(
                        &message,
                        // we could instead send back a message saying we don't have this key but I
                        // think this will never happen in practice unless we have a way for one
                        // coordinator to delete a key from a device without the other coordinator
                        // knowing.
                        format!("device doesn't have key for {key_id}"),
                    )
                })?;

                let checked_sign_task = sign_req
                    .sign_task
                    .clone()
                    .check(key.key_id())
                    .map_err(|e| Error::signer_invalid_message(&message, e))?;

                let key_id = key.key_id();
                let my_nonces = nonces.get(&key.index()).ok_or_else(|| {
                    Error::signer_invalid_message(
                        &message,
                        "this device was asked to sign but no nonces were provided",
                    )
                })?;

                let n_signatures_requested = checked_sign_task.sign_items().len();
                if my_nonces.nonces.len() != n_signatures_requested {
                    return Err(Error::signer_invalid_message(&message, format!("Number of nonces ({}) was not the same as the number of signatures we were asked for {}", my_nonces.nonces.len(), n_signatures_requested)));
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

                let agg_nonces = (0..n_signatures_requested).map(|i| sign_req.agg_nonce(i));

                self.action_state = Some(SignerState::AwaitingSignAck {
                    key_id,
                    sign_task: checked_sign_task.clone(),
                    agg_nonces: agg_nonces.collect(),
                    parties: sign_req.parties().collect(),
                    nonce_start_index: my_nonces.start,
                    nonces_remaining: my_nonces.nonces_remaining,
                });
                Ok(vec![DeviceSend::ToUser(
                    DeviceToUserMessage::SignatureRequest {
                        sign_task: checked_sign_task,
                        key_id,
                    },
                )])
            }
            (None, CoordinatorToDeviceMessage::DisplayBackup { key_id }) => {
                let _key = self
                    .secret_shares
                    .get(&key_id)
                    .ok_or(Error::signer_invalid_message(
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
            Some(SignerState::KeyGenAck {
                session_hash,
                secret_share,
            }) => {
                self.secret_shares
                    .insert(secret_share.key_id(), secret_share);
                Ok(vec![
                    DeviceSend::ToCoordinator(DeviceToCoordinatorMessage::KeyGenAck(session_hash)),
                    DeviceSend::ToStorage(DeviceToStorageMessage::SaveKey(secret_share)),
                    DeviceSend::ToUser(DeviceToUserMessage::DisplayBackup {
                        key_id: secret_share.key_id(),
                        backup: secret_share.secret_share().to_bech32_backup(),
                    }),
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
                agg_nonces,
                parties,
                nonce_start_index,
                nonces_remaining,
            }) => {
                let secret_share =
                    self.secret_shares
                        .get(key_id)
                        .ok_or(ActionError::StateInconsistent(format!(
                            "key {key_id} no longer exists so can't sign"
                        )))?;

                let sign_items = sign_task.sign_items();

                let new_nonces = {
                    // ⚠ Update nonce counter. Overflow would allow nonce reuse.
                    //
                    // hacktuallly this doesn't prevent nonce reuse. You can still re-use the nonce at
                    // u64::MAX. Leaving this bug in here intentionally to build test against later on.
                    //
                    self.nonce_counter = nonce_start_index.saturating_add(sign_items.len() as u64);

                    // This calculates the index after the last nonce the coordinator had. This is
                    // where we want to start providing new nonces.
                    let replenish_start = self.nonce_counter + *nonces_remaining;
                    // How many nonces we should give them from that point
                    let replenish_amount = NONCE_BATCH_SIZE.saturating_sub(*nonces_remaining);

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
                    .generate_nonces(*nonce_start_index)
                    .take(sign_items.len());

                let frost = frost::new_without_nonce_generation::<Sha256>();
                let mut signature_shares = vec![];

                for (signature_index, (sign_item, secret_nonce)) in
                    sign_items.iter().zip(secret_nonces).enumerate()
                {
                    let derived_xonly_key = sign_item.app_tweak.derive_xonly_key(secret_share);
                    let message = Message::raw(&sign_item.message[..]);
                    let sign_session = frost.party_sign_session(
                        derived_xonly_key.public_key(),
                        parties.clone(),
                        agg_nonces[signature_index],
                        message,
                    );
                    let sig_share = sign_session.sign(&derived_xonly_key, secret_nonce);

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
                let secret_share = self.secret_shares.get(&key_id).expect("key must exist");
                let backup = secret_share.secret_share().to_bech32_backup();

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
        device_to_share_index: BTreeMap<DeviceId, NonZeroU32>,
        input_state: encpedpop::Contributor,
        threshold: u16,
        key_name: String,
    },
    KeyGenAck {
        secret_share: PairedSecretShare,
        session_hash: SessionHash,
    },
    AwaitingSignAck {
        key_id: KeyId,
        sign_task: CheckedSignTask,
        agg_nonces: Vec<binonce::Nonce<Zero>>,
        parties: BTreeSet<PartyIndex>,
        nonce_start_index: u64,
        nonces_remaining: u64,
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
