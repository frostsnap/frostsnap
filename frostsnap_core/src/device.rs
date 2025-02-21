use crate::device_nonces::{self, AbSlots, MemoryNonceSlot, NonceStreamSlot};
use crate::symmetric_encryption::{Ciphertext, SymmetricKey};
use crate::tweak::{self, Xpub};
use crate::{
    bitcoin_transaction, message::*, AccessStructureId, AccessStructureRef, ActionError,
    CheckedSignTask, CoordShareDecryptionContrib, Error, KeyId, MessageResult, SessionHash,
    ShareImage,
};
use crate::{DeviceId, SignSessionId};
use alloc::boxed::Box;
use alloc::string::ToString as _;
use alloc::{
    collections::{BTreeMap, VecDeque},
    string::String,
    vec::Vec,
};
use core::num::NonZeroU32;
use schnorr_fun::frost::chilldkg::encpedpop;
use schnorr_fun::frost::{PairedSecretShare, PartyIndex, SecretShare};
use schnorr_fun::fun::KeyPair;
use schnorr_fun::fun::{g, G};
use schnorr_fun::{frost, fun::prelude::*, Message};

use sha2::Sha256;

#[derive(Clone, Debug, PartialEq)]
pub struct FrostSigner<S = MemoryNonceSlot> {
    keypair: KeyPair,
    keys: BTreeMap<KeyId, KeyData>,
    action_state: Option<SignerState>,
    nonce_slots: device_nonces::AbSlots<S>,
    mutations: VecDeque<Mutation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyData {
    access_structures: BTreeMap<AccessStructureId, AccessStructureData>,
    purpose: KeyPurpose,
    key_name: String,
}

/// In case we add access structures with more restricted properties later on
#[derive(Clone, Copy, Debug, PartialEq, bincode::Decode, bincode::Encode)]
pub enum AccessStructureKind {
    Master,
}

/// So the coordindator can recognise which keys are relevant to it
#[derive(Clone, Copy, Debug, PartialEq, bincode::Decode, bincode::Encode, Eq, PartialOrd, Ord)]
pub enum KeyPurpose {
    Test,
    Bitcoin(#[bincode(with_serde)] bitcoin::Network),
    Nostr,
}

impl KeyPurpose {
    pub fn bitcoin_network(&self) -> Option<bitcoin::Network> {
        match self {
            KeyPurpose::Bitcoin(network) => Some(*network),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, bincode::Decode, bincode::Encode)]
pub struct AccessStructureData {
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

impl<S: NonceStreamSlot + core::fmt::Debug> FrostSigner<S> {
    pub fn new(keypair: KeyPair, nonce_slots: AbSlots<S>) -> Self {
        Self {
            keypair,
            keys: Default::default(),
            action_state: None,
            nonce_slots,
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
                purpose,
            } => {
                self.keys.insert(
                    *key_id,
                    KeyData {
                        purpose: *purpose,
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
                        AccessStructureData {
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
        }
    }

    pub fn staged_mutations(&mut self) -> &mut VecDeque<Mutation> {
        &mut self.mutations
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

    pub fn recv_coordinator_message(
        &mut self,
        message: CoordinatorToDeviceMessage,
        rng: &mut impl rand_core::RngCore,
    ) -> MessageResult<Vec<DeviceSend>> {
        use CoordinatorToDeviceMessage::*;
        match (&self.action_state, message.clone()) {
            (_, OpenNonceStreams { streams }) => {
                let mut segments = vec![];
                for coord_stream_state in streams {
                    let slot = self
                        .nonce_slots
                        .get_or_create(coord_stream_state.stream_id, rng);
                    if let Some(segment) =
                        slot.reconcile_coord_nonce_stream_state(coord_stream_state)
                    {
                        segments.push(segment);
                    }
                }

                let send = if !segments.is_empty() {
                    Some(DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::NonceResponse { segments },
                    )))
                } else {
                    None
                };
                Ok(FromIterator::from_iter(send))
            }
            (
                None,
                DoKeyGen(super::DoKeyGen {
                    device_to_share_index,
                    threshold,
                    key_name,
                    purpose: key_purpose,
                }),
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
                    key_purpose,
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
                    key_purpose,
                    threshold,
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
                    .expect("this has been checked");
                let key_id = KeyId::from_rootkey(rootkey);

                let t_of_n = (*threshold, device_to_share_index.len() as u16);

                self.action_state = Some(SignerState::KeyGenAck {
                    secret_share,
                    agg_input,
                    key_name: key_name.clone(),
                    key_purpose: *key_purpose,
                });

                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::CheckKeyGen {
                        key_id,
                        session_hash,
                        key_name,
                        t_of_n,
                    },
                ))])
            }
            (
                None,
                CoordinatorToDeviceMessage::RequestSign(self::RequestSign {
                    group_sign_req,
                    device_sign_req,
                }),
            ) => {
                let session_id = group_sign_req.session_id();
                let group_sign_req = group_sign_req
                    .check(device_sign_req.rootkey)
                    .map_err(|e| Error::signer_invalid_message(&message, e))?;
                let GroupSignReq {
                    parties,
                    sign_task,
                    access_structure_id,
                    ..
                } = &group_sign_req;

                let DeviceSignReq {
                    nonces: coord_req_nonces,
                    rootkey,
                    ..
                } = device_sign_req;

                let key_id = KeyId::from_rootkey(rootkey);
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

                let access_structure_data =
                    key_data.access_structures.get(access_structure_id)
                        .ok_or_else( || {
                            Error::signer_invalid_message(
                                &message,
                                format!("this device is not part of that access structure: {access_structure_id}"),
                            )
                        })?.clone();

                // XXX We only support signing one share at a time for now
                let (my_party_index, encrypted_secret_share) = parties
                    .iter()
                    .find_map(|party| Some((*party, *access_structure_data.shares.get(party)?)))
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "device doesn't have any of the shares requested",
                        )
                    })?;

                let nonce_slot = self
                    .nonce_slots
                    .get(coord_req_nonces.stream_id)
                    .and_then(|slot| slot.read_slot())
                    .ok_or(Error::signer_invalid_message(
                        &message,
                        format!(
                            "device did not have that nonce stream id {}",
                            coord_req_nonces.stream_id
                        ),
                    ))?;

                if let Err(e) = nonce_slot.are_nonces_available(
                    coord_req_nonces.index,
                    group_sign_req.n_signatures().try_into().unwrap(),
                ) {
                    return Err(Error::signer_invalid_message(&message, e.to_string()));
                }

                let messages = vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::SignatureRequest {
                        sign_task: sign_task.clone(),
                    },
                ))];

                // we don't check anything more about nonces. The other error cases should be
                // unreachable so we can panic after confirmation.
                self.action_state = Some(SignerState::AwaitingSignAck(Box::new(AwaitingSignAck {
                    group_sign_req,
                    device_sign_req,
                    my_party_index,
                    encrypted_secret_share,
                    session_id,
                })));

                Ok(messages)
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
                    format!(
                        "signer doesn't have a share for this key: {}",
                        self.keys
                            .keys()
                            .map(|key| key.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
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
            (
                None,
                CoordinatorToDeviceMessage::VerifyAddress {
                    master_appkey,
                    derivation_index,
                },
            ) => {
                let key_id = master_appkey.key_id();
                // check we actually know about this key
                let _key_data = self
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

                let bip32_path = tweak::BitcoinBip32Path {
                    account_keychain: tweak::BitcoinAccountKeychain::external(),
                    index: derivation_index,
                };
                let spk = bitcoin_transaction::LocalSpk {
                    master_appkey,
                    bip32_path,
                };

                let network = self
                    .wallet_network(key_id)
                    .expect("cannot verify address on key that doesn't support bitcoin");

                let address =
                    bitcoin::Address::from_script(&spk.spk(), network).expect("has address form");

                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::VerifyAddress {
                        address,
                        bip32_path,
                    },
                ))])
            }
            (None, CoordinatorToDeviceMessage::CheckShareBackup) => {
                self.action_state = Some(SignerState::LoadingBackup);
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::EnterBackup,
                ))])
            }
            (_, CoordinatorToDeviceMessage::RequestHeldShares) => {
                let held_shares = self.held_shares();
                let send = if !held_shares.is_empty() {
                    Some(DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::HeldShares(held_shares),
                    )))
                } else {
                    None
                };
                Ok(FromIterator::from_iter(send))
            }
            _ => Err(Error::signer_message_kind(&self.action_state, &message)),
        }
    }

    pub fn keygen_ack(
        &mut self,
        symm_key_gen: &mut impl DeviceSymmetricKeyGen,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<KeyGenAck, ActionError> {
        match self.action_state.take() {
            Some(SignerState::KeyGenAck {
                agg_input,
                secret_share,
                key_name,
                key_purpose,
            }) => {
                let rootkey = secret_share.public_key();
                let key_id = KeyId::from_rootkey(rootkey);
                let root_shared_key =
                    Xpub::from_rootkey(agg_input.shared_key().non_zero().expect("already checked"));
                let app_shared_key = root_shared_key.rootkey_to_master_appkey();

                let access_structure_id =
                    AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());
                let decryption_share_contrib = CoordShareDecryptionContrib::for_master_share(
                    self.device_id(),
                    &root_shared_key.key,
                );
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
                    purpose: key_purpose,
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
                Ok(KeyGenAck { session_hash })
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
                    device_sign_req:
                        DeviceSignReq {
                            nonces: coord_nonce_state,
                            rootkey,
                            coord_share_decryption_contrib,
                        },
                    group_sign_req:
                        GroupSignReq {
                            parties,
                            agg_nonces,
                            sign_task,
                            access_structure_id,
                        },
                    encrypted_secret_share,
                    my_party_index,
                    session_id,
                } = *boxed;
                let sign_items = sign_task.sign_items();
                let key_id = KeyId::from_rootkey(rootkey);

                let symmetric_key = symm_key_gen.get_share_encryption_key(
                    key_id,
                    access_structure_id,
                    my_party_index,
                    coord_share_decryption_contrib,
                );
                let secret_share = encrypted_secret_share
                    .ciphertext
                    .decrypt(symmetric_key)
                    .ok_or_else(|| {
                        ActionError::StateInconsistent("couldn't decrypt secret share".into())
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

                let frost = frost::new_without_nonce_generation::<sha2::Sha256>();

                let sign_sessions =
                    sign_items
                        .iter()
                        .enumerate()
                        .map(|(signature_index, sign_item)| {
                            let derived_xonly_key = sign_item
                                .app_tweak
                                .derive_xonly_key(&app_paired_secret_share);
                            let message = Message::raw(&sign_item.message[..]);
                            let session = frost.party_sign_session(
                                derived_xonly_key.public_key(),
                                parties.clone(),
                                agg_nonces[signature_index],
                                message,
                            );

                            (derived_xonly_key, session)
                        });

                let (signature_shares, replenish_nonces) = self
                    .nonce_slots
                    .sign_guaranteeing_nonces_destroyed(
                        session_id,
                        coord_nonce_state,
                        sign_sessions,
                    )
                    .expect("nonce stream already checked to exist");

                self.action_state = None;

                Ok(vec![DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::SignatureShare {
                        session_id,
                        signature_shares,
                        replenish_nonces,
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

    pub fn held_shares(&self) -> Vec<HeldShare> {
        let mut held_shares = vec![];

        for (key_id, key_data) in &self.keys {
            for (access_structure_id, access_structure) in &key_data.access_structures {
                for (share_index, share) in &access_structure.shares {
                    if access_structure.kind == AccessStructureKind::Master {
                        held_shares.push(HeldShare {
                            key_name: key_data.key_name.clone(),
                            share_image: ShareImage {
                                point: share.image,
                                share_index: *share_index,
                            },
                            access_structure_ref: AccessStructureRef {
                                access_structure_id: *access_structure_id,
                                key_id: *key_id,
                            },
                            threshold: access_structure.threshold,
                            purpose: key_data.purpose,
                        });
                    }
                }
            }
        }
        held_shares
    }

    pub fn action_state_name(&self) -> &'static str {
        self.action_state
            .as_ref()
            .map(|x| x.name())
            .unwrap_or("None")
    }

    pub fn wallet_network(&self, key_id: KeyId) -> Option<bitcoin::Network> {
        self.keys.get(&key_id).and_then(|key| match key.purpose {
            KeyPurpose::Bitcoin(network) => Some(network),
            _ => None,
        })
    }

    /// This is for inspecting the state of the nonce slots for testing.
    /// Never to be used in production.
    pub fn nonce_slots(&mut self) -> &mut AbSlots<S> {
        &mut self.nonce_slots
    }
}

impl FrostSigner<MemoryNonceSlot> {
    pub fn new_random(rng: &mut impl rand_core::RngCore) -> Self {
        Self::new(
            KeyPair::<Normal>::new(Scalar::random(rng)),
            AbSlots::new((0..8).map(|_| MemoryNonceSlot::default()).collect()),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AwaitingSignAck {
    pub group_sign_req: GroupSignReq<CheckedSignTask>,
    pub device_sign_req: DeviceSignReq,
    pub my_party_index: PartyIndex,
    pub encrypted_secret_share: EncryptedSecretShare,
    pub session_id: SignSessionId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SignerState {
    KeyGen {
        device_to_share_index: BTreeMap<DeviceId, NonZeroU32>,
        input_state: encpedpop::Contributor,
        threshold: u16,
        key_name: String,
        key_purpose: KeyPurpose,
    },
    KeyGenAck {
        secret_share: PairedSecretShare,
        agg_input: encpedpop::AggKeygenInput,
        key_name: String,
        key_purpose: KeyPurpose,
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
        purpose: KeyPurpose,
    },
    NewAccessStructure {
        key_id: KeyId,
        access_structure_id: AccessStructureId,
        threshold: u16,
        kind: AccessStructureKind,
    },
    SaveShare(Box<SaveShareMutation>),
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

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct KeyGenAck {
    pub session_hash: SessionHash,
}

impl IntoIterator for KeyGenAck {
    type Item = DeviceSend;
    type IntoIter = core::iter::Once<DeviceSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(DeviceSend::ToCoordinator(Box::new(self.into())))
    }
}

impl From<KeyGenAck> for DeviceToCoordinatorMessage {
    fn from(value: KeyGenAck) -> Self {
        DeviceToCoordinatorMessage::KeyGenAck(value.session_hash)
    }
}
