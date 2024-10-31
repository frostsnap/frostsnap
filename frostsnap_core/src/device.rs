use core::num::NonZeroU32;

use crate::symmetric_encryption::{Ciphertext, SymmetricKey};
use crate::tweak::Xpub;
use crate::{
    message::*, AccessStructureId, ActionError, CheckedSignTask, CoordShareDecryptionContrib,
    Error, KeyId, MessageResult, SessionHash, NONCE_BATCH_SIZE,
};
use crate::{DeviceId, MasterAppkey};
use alloc::boxed::Box;
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::String,
    vec::Vec,
};
use rand_chacha::ChaCha20Rng;
use schnorr_fun::binonce;
use schnorr_fun::frost::chilldkg::encpedpop;
use schnorr_fun::frost::{PairedSecretShare, PartyIndex, SecretShare};
use schnorr_fun::fun::KeyPair;
use schnorr_fun::fun::{g, G};
use schnorr_fun::{
    binonce::{Nonce, NonceKeyPair},
    frost::{self},
    fun::{derive_nonce_rng, prelude::*, Tag},
    nonce, Message,
};

use sha2::Sha256;

#[derive(Clone, Debug)]
pub struct FrostSigner {
    keypair: KeyPair,
    keys: BTreeMap<KeyId, KeyData>,
    action_state: Option<SignerState>,
    nonce_counter: u64,
    mutations: VecDeque<Mutation>,
}

#[derive(Clone, Debug)]
pub struct KeyData {
    access_structures: BTreeMap<AccessStructureId, AccessStrucureData>,
    #[allow(dead_code)] // We'll use this soon
    purposes: BTreeSet<KeyPurpose>,
    key_name: String,
}

/// In case we add access structures with more restricted properties later on
#[derive(Clone, Copy, Debug, PartialEq, bincode::Decode, bincode::Encode)]
pub enum AccessStructureKind {
    Master,
}

/// So the coorindator can recognise which keys are relevant to it
#[derive(Clone, Copy, Debug, PartialEq, bincode::Decode, bincode::Encode, Eq, PartialOrd, Ord)]
pub enum KeyPurpose {
    Test,
    Bitcoin,
    Nostr,
}

impl KeyPurpose {
    pub fn all() -> impl Iterator<Item = KeyPurpose> {
        use KeyPurpose::*;
        [Test, Bitcoin, Nostr].into_iter()
    }
}

#[derive(Clone, Debug, PartialEq, bincode::Decode, bincode::Encode)]
pub struct AccessStrucureData {
    pub kind: AccessStructureKind,
    /// Keep the threshold around to make recover easier. The device tells the coordinator about it
    /// so they can tell the user how close they are to restoring the key.
    pub threshold: u16,
    pub shares: BTreeMap<PartyIndex, EncryptedSecretShare>,
}

#[derive(Clone, Copy, Debug, PartialEq, bincode::Decode, bincode::Encode)]
pub struct EncryptedSecretShare {
    /// The image of the encrypted secret. The device stores this so it can tell the coordinator
    /// about it as part of the recovery system.
    pub image: Point<Normal, Public, Zero>,
    /// The encrypted secret share
    pub ciphertext: Ciphertext<32, Scalar<Secret, Zero>>,
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
            NewKey {
                key_id,
                key_name,
                purposes,
            } => {
                self.keys.insert(
                    *key_id,
                    KeyData {
                        purposes: purposes.clone(),
                        access_structures: Default::default(),
                        key_name: key_name.into(),
                    },
                );
            }
            NewAccessStructure {
                key_id,
                kind,
                access_structure_id,
                threshold,
            } => {
                self.keys.entry(*key_id).and_modify(|key_data| {
                    key_data.access_structures.insert(
                        *access_structure_id,
                        AccessStrucureData {
                            kind: *kind,
                            threshold: *threshold,
                            shares: Default::default(),
                        },
                    );
                });
            }
            SaveShare(boxed) => {
                let SaveShareMutation {
                    key_id,
                    access_structure_id,
                    party_index,
                    encrypted_secret_share,
                } = boxed.as_ref();
                self.keys.entry(*key_id).and_modify(|key_data| {
                    key_data
                        .access_structures
                        .entry(*access_structure_id)
                        .and_modify(|access_structure_data| {
                            access_structure_data
                                .shares
                                .insert(*party_index, *encrypted_secret_share);
                        });
                });
            }
            ExpendNonce { nonce_counter } => {
                self.nonce_counter = self.nonce_counter.max(*nonce_counter);
            }
        }
    }

    pub fn take_staged_mutations(&mut self) -> VecDeque<Mutation> {
        core::mem::take(&mut self.mutations)
    }

    #[must_use]
    pub fn cancel_action(&mut self) -> Option<DeviceSend> {
        let task = match self.action_state.take()? {
            SignerState::KeyGen { .. } | SignerState::KeyGenAck { .. } => TaskKind::KeyGen,
            SignerState::AwaitingSignAck { .. } => TaskKind::Sign,
            SignerState::LoadingBackup { .. } => TaskKind::CheckBackup,
            SignerState::AwaitingDisplayBackupAck { .. } => TaskKind::DisplayBackup,
        };

        Some(DeviceSend::ToUser(Box::new(
            DeviceToUserMessage::Canceled { task },
        )))
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
        match (&self.action_state, message.clone()) {
            (_, RequestNonces) => {
                let nonces = self
                    .generate_nonces(self.nonce_counter)
                    .take(NONCE_BATCH_SIZE as usize)
                    .map(|nonce| nonce.public())
                    .collect();

                Ok(vec![DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::NonceResponse(DeviceNonces {
                        start_index: self.nonce_counter,
                        nonces,
                    }),
                ))])
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

                Ok(vec![DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::KeyGenResponse(keygen_input),
                ))])
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

                let session_hash = SessionHash::from_agg_input(&agg_input);
                let rootkey = agg_input
                    .shared_key()
                    .public_key()
                    .non_zero()
                    .expect("this has beeen checked");
                let key_id = KeyId::from_rootkey(rootkey);

                self.action_state = Some(SignerState::KeyGenAck {
                    secret_share,
                    agg_input,
                    key_name: key_name.clone(),
                });

                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::CheckKeyGen {
                        key_id,
                        session_hash,
                        key_name,
                    },
                ))])
            }
            (None, CoordinatorToDeviceMessage::RequestSign(sign_req)) => {
                let rootkey = sign_req.rootkey;
                let key_id = KeyId::from_rootkey(rootkey);
                let access_structure_id = sign_req.access_structure_id;
                let nonces = sign_req.nonces.clone();
                let key_data = self
                    .keys
                    .get(&key_id)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            // we could instead send back a message saying we don't have this key but I
                            // think this will never happen in practice unless we have a way for one
                            // coordinator to delete a key from a device without the other coordinator
                            // knowing.
                            format!("device doesn't have key for {key_id}"),
                        )
                    })?
                    .clone();

                let master_appkey = MasterAppkey::derive_from_rootkey(rootkey);
                let access_structure_data =
                    key_data.access_structures.get(&access_structure_id)
                        .ok_or_else( || {
                            Error::signer_invalid_message(
                                &message,
                                format!("this device is not part of that access structure: {access_structure_id}"),
                            )
                        })?.clone();

                // XXX We only support signing one share at a time for now
                let (party_index, encrypted_secret_share) = sign_req
                    .parties()
                    .find_map(|party| Some((party, access_structure_data.shares.get(&party)?)))
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "device doesn't have any of the shares requested",
                        )
                    })?;

                let checked_sign_task = sign_req
                    .sign_task
                    .clone()
                    .check(master_appkey)
                    .map_err(|e| Error::signer_invalid_message(&message, e))?;

                let my_nonces = nonces.get(&party_index).ok_or_else(|| {
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

                self.action_state = Some(SignerState::AwaitingSignAck(Box::new(AwaitingSignAck {
                    rootkey,
                    access_structure_id,
                    encrypted_secret_share: encrypted_secret_share.ciphertext,
                    my_party_index: party_index,
                    sign_task: checked_sign_task.clone(),
                    agg_nonces: agg_nonces.collect(),
                    parties: sign_req.parties().collect(),
                    nonce_start_index: my_nonces.start,
                    nonces_remaining: my_nonces.nonces_remaining,
                    coord_share_decryption_contrib: sign_req.coord_share_decryption_contrib,
                })));
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::SignatureRequest {
                        sign_task: checked_sign_task,
                        master_appkey,
                    },
                ))])
            }
            (
                None,
                CoordinatorToDeviceMessage::DisplayBackup {
                    key_id,
                    access_structure_id,
                    coord_share_decryption_contrib,
                    party_index,
                },
            ) => {
                let key_data = self.keys.get(&key_id).ok_or(Error::signer_invalid_message(
                    &message,
                    "signer doesn't have a share for this key",
                ))?;

                let access_structure_data = key_data
                    .access_structures
                    .get(&access_structure_id)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "no such access structure on this device",
                        )
                    })?;

                let encrypted_secret_share = access_structure_data
                    .shares
                    .get(&party_index)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "access structure exists but this device doesn't have that share",
                        )
                    })?
                    .ciphertext;

                self.action_state = Some(SignerState::AwaitingDisplayBackupAck {
                    party_index,
                    key_id,
                    access_structure_id,
                    encrypted_secret_share,
                    coord_share_decryption_contrib,
                });

                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::DisplayBackupRequest {
                        key_name: key_data.key_name.clone(),
                        key_id,
                    },
                ))])
            }
            (None, CoordinatorToDeviceMessage::CheckShareBackup) => {
                self.action_state = Some(SignerState::LoadingBackup);
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::EnterBackup,
                ))])
            }
            _ => Err(Error::signer_message_kind(&self.action_state, &message)),
        }
    }

    pub fn keygen_ack(
        &mut self,
        symm_key_gen: &mut impl DeviceSymmetricKeyGen,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<Vec<DeviceSend>, ActionError> {
        match self.action_state.take() {
            Some(SignerState::KeyGenAck {
                agg_input,
                secret_share,
                key_name,
            }) => {
                let rootkey = secret_share.public_key();
                let key_id = KeyId::from_rootkey(rootkey);
                let root_shared_key =
                    Xpub::from_rootkey(agg_input.shared_key().non_zero().expect("already checked"));
                let app_shared_key = root_shared_key.rootkey_to_master_appkey();

                let access_structure_id =
                    AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());
                let decryption_share_contrib =
                    CoordShareDecryptionContrib::from_root_shared_key(&root_shared_key.key);
                let encryption_key = symm_key_gen.get_share_encryption_key(
                    key_id,
                    access_structure_id,
                    secret_share.index(),
                    decryption_share_contrib,
                );
                let encrypted_secret =
                    Ciphertext::encrypt(encryption_key, &secret_share.secret_share().share, rng);
                let session_hash = SessionHash::from_agg_input(&agg_input);

                // XXX: order is important here
                self.mutate(Mutation::NewKey {
                    key_id,
                    key_name: key_name.clone(),
                    purposes: KeyPurpose::all().collect(),
                });
                self.mutate(Mutation::NewAccessStructure {
                    key_id,
                    access_structure_id,
                    threshold: app_shared_key
                        .key
                        .threshold()
                        .try_into()
                        .expect("threshold was too large"),
                    kind: AccessStructureKind::Master,
                });
                self.mutate(Mutation::SaveShare(Box::new(SaveShareMutation {
                    key_id,
                    party_index: secret_share.index(),
                    access_structure_id,
                    encrypted_secret_share: EncryptedSecretShare {
                        image: secret_share.secret_share().share_image().normalize(),
                        ciphertext: encrypted_secret,
                    },
                })));
                Ok(vec![DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::KeyGenAck(session_hash),
                ))])
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

    pub fn sign_ack(
        &mut self,
        symm_key_gen: &mut impl DeviceSymmetricKeyGen,
    ) -> Result<Vec<DeviceSend>, ActionError> {
        match self.action_state.take() {
            Some(SignerState::AwaitingSignAck(boxed)) => {
                let AwaitingSignAck {
                    rootkey,
                    access_structure_id,
                    encrypted_secret_share,
                    my_party_index,
                    sign_task,
                    agg_nonces,
                    parties,
                    nonce_start_index,
                    nonces_remaining,
                    coord_share_decryption_contrib,
                } = *boxed;
                let sign_items = sign_task.sign_items();

                let new_nonces = {
                    // ⚠ Update nonce counter. Overflow would allow nonce reuse.
                    //
                    // hacktuallly this doesn't prevent nonce reuse. You can still re-use the nonce at
                    // u64::MAX. Leaving this bug in here intentionally to build test against later on.

                    self.mutate(Mutation::ExpendNonce {
                        nonce_counter: nonce_start_index.saturating_add(sign_items.len() as u64),
                    });

                    // This calculates the index after the last nonce the coordinator had. This is
                    // where we want to start providing new nonces.
                    let replenish_start = self.nonce_counter + nonces_remaining;
                    // How many nonces we should give them from that point
                    let replenish_amount = NONCE_BATCH_SIZE.saturating_sub(nonces_remaining);

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
                    .generate_nonces(nonce_start_index)
                    .take(sign_items.len());

                let frost = frost::new_without_nonce_generation::<Sha256>();
                let mut signature_shares = vec![];

                let key_id = KeyId::from_rootkey(rootkey);
                let symmetric_key = symm_key_gen.get_share_encryption_key(
                    key_id,
                    access_structure_id,
                    my_party_index,
                    coord_share_decryption_contrib,
                );
                let secret_share =
                    encrypted_secret_share
                        .decrypt(symmetric_key)
                        .ok_or_else(|| {
                            ActionError::StateInconsistent("couldn't decrypt secrert share".into())
                        })?;
                let root_paired_secret_share =
                    Xpub::from_rootkey(PairedSecretShare::new_unchecked(
                        SecretShare {
                            index: my_party_index,
                            share: secret_share,
                        },
                        rootkey,
                    ));
                let app_paired_secret_share = root_paired_secret_share.rootkey_to_master_appkey();

                for (signature_index, (sign_item, secret_nonce)) in
                    sign_items.iter().zip(secret_nonces).enumerate()
                {
                    let derived_xonly_key = sign_item
                        .app_tweak
                        .derive_xonly_key(&app_paired_secret_share);
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

                Ok(vec![DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::SignatureShare {
                        signature_shares,
                        new_nonces,
                    },
                ))])
            }
            _ => Err(ActionError::WrongState {
                in_state: self.action_state_name(),
                action: "sign_ack",
            }),
        }
    }

    pub fn display_backup_ack(
        &mut self,
        symm_key_gen: &mut impl DeviceSymmetricKeyGen,
    ) -> Result<Vec<DeviceSend>, ActionError> {
        match self.action_state {
            Some(SignerState::AwaitingDisplayBackupAck {
                key_id,
                access_structure_id,
                party_index,
                encrypted_secret_share,
                coord_share_decryption_contrib,
            }) => {
                self.action_state = None;
                let key_data = self.keys.get(&key_id).expect("key must exist");
                let encryption_key = symm_key_gen.get_share_encryption_key(
                    key_id,
                    access_structure_id,
                    party_index,
                    coord_share_decryption_contrib,
                );

                let secret_share = encrypted_secret_share.decrypt(encryption_key).ok_or(
                    ActionError::StateInconsistent("could not decrypt secret share".into()),
                )?;
                let backup = SecretShare {
                    index: party_index,
                    share: secret_share,
                }
                .to_bech32_backup();

                Ok(vec![
                    DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::DisplayBackupConfirmed,
                    )),
                    DeviceSend::ToUser(Box::new(DeviceToUserMessage::DisplayBackup {
                        key_name: key_data.key_name.clone(),
                        backup,
                    })),
                ])
            }
            _ => Err(ActionError::WrongState {
                in_state: self.action_state_name(),
                action: "display_backup_ack",
            }),
        }
    }

    pub fn loaded_share_backup(
        &mut self,
        share_backup: SecretShare,
    ) -> Result<Vec<DeviceSend>, ActionError> {
        if !matches!(self.action_state, Some(SignerState::LoadingBackup)) {
            return Err(ActionError::WrongState {
                in_state: self.action_state_name(),
                action: "loaded_share_backup",
            });
        }

        self.action_state = None;

        let share_point = g!(share_backup.share * G).normalize();

        Ok(vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::CheckShareBackup {
                share_image: ShareImage {
                    point: share_point,
                    share_index: share_backup.index,
                },
            },
        ))])
    }

    pub fn action_state_name(&self) -> &'static str {
        self.action_state
            .as_ref()
            .map(|x| x.name())
            .unwrap_or("None")
    }
}

#[derive(Clone, Debug)]
pub struct AwaitingSignAck {
    pub rootkey: Point,
    pub access_structure_id: AccessStructureId,
    pub encrypted_secret_share: Ciphertext<32, Scalar<Secret, Zero>>,
    pub my_party_index: PartyIndex,
    pub sign_task: CheckedSignTask,
    pub agg_nonces: Vec<binonce::Nonce<Zero>>,
    pub parties: BTreeSet<PartyIndex>,
    pub nonce_start_index: u64,
    pub nonces_remaining: u64,
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
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
        agg_input: encpedpop::AggKeygenInput,
        key_name: String,
    },
    AwaitingSignAck(Box<AwaitingSignAck>),
    AwaitingDisplayBackupAck {
        key_id: KeyId,
        access_structure_id: AccessStructureId,
        party_index: PartyIndex,
        encrypted_secret_share: Ciphertext<32, Scalar<Secret, Zero>>,
        coord_share_decryption_contrib: CoordShareDecryptionContrib,
    },
    LoadingBackup,
}

impl SignerState {
    pub fn name(&self) -> &'static str {
        match self {
            SignerState::KeyGen { .. } => "KeyGen",
            SignerState::KeyGenAck { .. } => "KeyGenAck",
            SignerState::LoadingBackup { .. } => "LoadingBackup",
            SignerState::AwaitingSignAck(_) => "AwaitingSignAck",
            SignerState::AwaitingDisplayBackupAck { .. } => "AwaitingDisplayBackupAck",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SaveShareMutation {
    pub key_id: KeyId,
    pub access_structure_id: AccessStructureId,
    pub party_index: PartyIndex,
    pub encrypted_secret_share: EncryptedSecretShare,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub enum Mutation {
    NewKey {
        key_id: KeyId,
        key_name: String,
        purposes: BTreeSet<KeyPurpose>,
    },
    NewAccessStructure {
        key_id: KeyId,
        access_structure_id: AccessStructureId,
        threshold: u16,
        kind: AccessStructureKind,
    },
    SaveShare(Box<SaveShareMutation>),
    ExpendNonce {
        nonce_counter: u64,
    },
}

pub trait DeviceSymmetricKeyGen {
    fn get_share_encryption_key(
        &mut self,
        key_id: KeyId,
        access_structure_id: AccessStructureId,
        party_index: PartyIndex,
        coord_key: CoordShareDecryptionContrib,
    ) -> SymmetricKey;
}
