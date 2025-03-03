use crate::device_nonces::{self, AbSlots, MemoryNonceSlot, NonceStreamSlot};
use crate::symmetric_encryption::{Ciphertext, SymmetricKey};
use crate::tweak::{self, Xpub};
use crate::{
    bitcoin_transaction, message::*, AccessStructureId, AccessStructureRef, ActionError,
    CheckedSignTask, CoordShareDecryptionContrib, Error, KeyId, KeygenId, MessageResult,
    SessionHash, ShareImage,
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
use schnorr_fun::frost::chilldkg::encpedpop::{self};
use schnorr_fun::frost::{PairedSecretShare, PartyIndex, SecretShare};
use schnorr_fun::fun::KeyPair;
use schnorr_fun::fun::{g, G};
use schnorr_fun::{frost, fun::prelude::*};

use sha2::Sha256;

#[derive(Clone, Debug, PartialEq)]
pub struct FrostSigner<S = MemoryNonceSlot> {
    keypair: KeyPair,
    keys: BTreeMap<KeyId, KeyData>,
    nonce_slots: device_nonces::AbSlots<S>,
    mutations: VecDeque<Mutation>,
    keygen_phase1: BTreeMap<KeygenId, KeyGenPhase1>,
    tmp_loaded_backups: Vec<CompleteSecretShare>,
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

#[derive(Clone, Debug, PartialEq)]
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
    pub share_image: ShareImage,
    /// The encrypted secret share
    pub ciphertext: Ciphertext<32, Scalar<Secret, Zero>>,
}

impl EncryptedSecretShare {
    pub fn encrypt(
        secret_share: SecretShare,
        access_structure_ref: AccessStructureRef,
        coord_contrib: CoordShareDecryptionContrib,
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
        rng: &mut impl rand_core::RngCore,
    ) -> Self {
        let share_image = ShareImage::from_secret(secret_share);
        let encryption_key = symm_keygen.get_share_encryption_key(
            access_structure_ref,
            secret_share.index,
            coord_contrib,
        );
        let ciphertext = Ciphertext::encrypt(encryption_key, &secret_share.share, rng);
        EncryptedSecretShare {
            share_image,
            ciphertext,
        }
    }

    pub fn decrypt(
        &self,
        access_structure_ref: AccessStructureRef,
        coord_contrib: CoordShareDecryptionContrib,
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
    ) -> Option<SecretShare> {
        let encryption_key = symm_keygen.get_share_encryption_key(
            access_structure_ref,
            self.share_image.share_index,
            coord_contrib,
        );

        self.ciphertext
            .decrypt(encryption_key)
            .map(|share| SecretShare {
                index: self.share_image.share_index,
                share,
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenPhase1 {
    pub device_to_share_index: BTreeMap<DeviceId, NonZeroU32>,
    pub input_state: encpedpop::Contributor,
    pub threshold: u16,
    pub key_name: String,
    pub key_purpose: KeyPurpose,
}

#[derive(Clone, Debug)]
pub struct KeyGenPhase2 {
    pub keygen_id: KeygenId,
    secret_share: PairedSecretShare,
    agg_input: encpedpop::AggKeygenInput,
    key_name: String,
    key_purpose: KeyPurpose,
}

impl KeyGenPhase2 {
    pub fn session_hash(&self) -> SessionHash {
        SessionHash::from_agg_input(&self.agg_input)
    }
    pub fn key_name(&self) -> &str {
        self.key_name.as_str()
    }
    pub fn t_of_n(&self) -> (u16, u16) {
        (
            self.agg_input.shared_key().threshold().try_into().unwrap(),
            self.agg_input.encryption_keys().count().try_into().unwrap(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct BackupDisplayPhase {
    pub access_structure_ref: AccessStructureRef,
    pub party_index: PartyIndex,
    pub encrypted_secret_share: Ciphertext<32, Scalar<Secret, Zero>>,
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
    pub key_name: String,
}

pub type LoadKnownBackupPhase = LoadKnownBackup;

#[derive(Clone, Debug, PartialEq)]
pub struct SignPhase1 {
    group_sign_req: GroupSignReq<CheckedSignTask>,
    device_sign_req: DeviceSignReq,
    encrypted_secret_share: EncryptedSecretShare,
    session_id: SignSessionId,
}

impl SignPhase1 {
    pub fn sign_task(&self) -> &CheckedSignTask {
        &self.group_sign_req.sign_task
    }
}

impl<S: NonceStreamSlot + core::fmt::Debug> FrostSigner<S> {
    pub fn new(keypair: KeyPair, nonce_slots: AbSlots<S>) -> Self {
        Self {
            keypair,
            keys: Default::default(),
            nonce_slots,
            mutations: Default::default(),
            keygen_phase1: Default::default(),
            tmp_loaded_backups: Default::default(),
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
                    encrypted_secret_share,
                } = boxed.as_ref();
                self.keys.entry(*key_id).and_modify(|key_data| {
                    key_data
                        .access_structures
                        .entry(*access_structure_id)
                        .and_modify(|access_structure_data| {
                            access_structure_data.shares.insert(
                                encrypted_secret_share.share_image.share_index,
                                *encrypted_secret_share,
                            );
                        });
                });
            }
        }
    }

    pub fn staged_mutations(&mut self) -> &mut VecDeque<Mutation> {
        &mut self.mutations
    }

    pub fn clear_unfinished_keygens(&mut self) {
        self.keygen_phase1.clear();
    }

    pub fn clear_loaded_backups(&mut self) {
        self.tmp_loaded_backups.clear();
    }
    pub fn clear_tmp_data(&mut self) {
        self.clear_unfinished_keygens();
        self.clear_loaded_backups();
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
        match message.clone() {
            OpenNonceStreams { streams } => {
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
                Ok(send.into_iter().collect())
            }
            DoKeyGen(self::DoKeyGen {
                keygen_id,
                device_to_share_index,
                threshold,
                key_name,
                purpose: key_purpose,
            }) => {
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
                self.keygen_phase1.insert(
                    keygen_id,
                    KeyGenPhase1 {
                        device_to_share_index,
                        input_state,
                        threshold,
                        key_name: key_name.clone(),
                        key_purpose,
                    },
                );
                Ok(vec![DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::KeyGenResponse(KeyGenResponse {
                        keygen_id,
                        input: keygen_input,
                    }),
                ))])
            }
            FinishKeyGen {
                keygen_id,
                agg_input,
            } => {
                let phase1 = self.keygen_phase1.remove(&keygen_id).ok_or_else(|| {
                    Error::signer_invalid_message(
                        &message,
                        "no keygen state for provided keygen_id",
                    )
                })?;
                phase1
                    .input_state
                    .verify_agg_input(&agg_input)
                    .map_err(|e| Error::signer_invalid_message(&message, e))?;

                let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
                // Note we could just store my_index in our state. But we want to keep aroudn the
                // keys of other devices for when we move to a certification based keygen.
                let my_index = phase1
                    .device_to_share_index
                    .get(&self.device_id())
                    .expect("already checked");

                let secret_share = encpedpop::receive_share(
                    &schnorr,
                    (*my_index).into(),
                    &self.keypair,
                    &agg_input,
                )
                .map_err(|e| Error::signer_invalid_message(&message, e))?
                .non_zero()
                .ok_or_else(|| {
                    Error::signer_invalid_message(&message, "keygen produced a zero shared key")
                })?;

                let phase2 = KeyGenPhase2 {
                    keygen_id,
                    secret_share,
                    agg_input,
                    key_name: phase1.key_name.clone(),
                    key_purpose: phase1.key_purpose,
                };
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::CheckKeyGen {
                        phase: Box::new(phase2),
                    },
                ))])
            }
            RequestSign(self::RequestSign {
                group_sign_req,
                device_sign_req,
            }) => {
                let session_id = group_sign_req.session_id();
                let key_id = KeyId::from_rootkey(device_sign_req.rootkey);
                let key_data = self
                    .keys
                    .get(&key_id)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            format!("device doesn't have key for {key_id}"),
                        )
                    })?
                    .clone();

                let group_sign_req = group_sign_req
                    .check(device_sign_req.rootkey, key_data.purpose)
                    .map_err(|e| Error::signer_invalid_message(&message, e))?;

                let GroupSignReq {
                    parties,
                    access_structure_id,
                    ..
                } = &group_sign_req;
                let coord_req_nonces = device_sign_req.nonces;

                let access_structure_data = key_data.access_structures.get(access_structure_id)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(&message, format!("this device is not part of that access structure: {access_structure_id}"))
                    })?.clone();
                let (_, encrypted_secret_share) = parties
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
                let phase = SignPhase1 {
                    group_sign_req,
                    device_sign_req,
                    encrypted_secret_share,
                    session_id,
                };
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::SignatureRequest {
                        phase: Box::new(phase),
                    },
                ))])
            }
            DisplayBackup {
                access_structure_ref,
                coord_share_decryption_contrib,
                party_index,
            } => {
                let AccessStructureRef {
                    key_id,
                    access_structure_id,
                } = access_structure_ref;
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
                let phase = BackupDisplayPhase {
                    access_structure_ref,
                    party_index,
                    encrypted_secret_share,
                    coord_share_decryption_contrib,
                    key_name: key_data.key_name.clone(),
                };
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::DisplayBackupRequest {
                        phase: Box::new(phase),
                    },
                ))])
            }
            VerifyAddress {
                master_appkey,
                derivation_index,
            } => {
                let key_id = master_appkey.key_id();
                // check we actually know about this key
                let _key_data = self
                    .keys
                    .get(&key_id)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
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
            LoadKnownBackup(load_known_backup) => Ok(vec![DeviceSend::ToUser(Box::new(
                DeviceToUserMessage::EnterBackup {
                    phase: load_known_backup,
                },
            ))]),
            RequestHeldShares => {
                let held_shares = self.held_shares();
                let send = if !held_shares.is_empty() {
                    Some(DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::HeldShares(held_shares),
                    )))
                } else {
                    None
                };
                Ok(send.into_iter().collect())
            }
        }
    }

    pub fn keygen_ack(
        &mut self,
        phase2: KeyGenPhase2,
        symm_key_gen: &mut impl DeviceSymmetricKeyGen,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<KeyGenAck, ActionError> {
        let secret_share = phase2.secret_share;
        let agg_input = phase2.agg_input;
        let key_name = phase2.key_name;
        let rootkey = secret_share.public_key();
        let key_id = KeyId::from_rootkey(rootkey);
        let root_shared_key =
            Xpub::from_rootkey(agg_input.shared_key().non_zero().expect("already checked"));
        let app_shared_key = root_shared_key.rootkey_to_master_appkey();
        let access_structure_id =
            AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());

        // SHARE ENCRYPTION NOTE 1: We make the device gnerate the encryption key for the share right after keygen rather
        // than letting the coordinator send it to the device to protect against malicious
        // coordinators. A coordinator could provide garbage for example and then the device would
        // never be able to decrypt its share again.
        let decryption_share_contrib =
            CoordShareDecryptionContrib::for_master_share(self.device_id(), &root_shared_key.key);

        let session_hash = SessionHash::from_agg_input(&agg_input);
        let threshold = app_shared_key
            .key
            .threshold()
            .try_into()
            .expect("threshold was too large");

        let complete_share = CompleteSecretShare {
            access_structure_ref: AccessStructureRef {
                key_id,
                access_structure_id,
            },
            key_name,
            purpose: phase2.key_purpose,
            threshold,
            secret_share: *secret_share.secret_share(),
        };

        self.save_complete_share(complete_share, symm_key_gen, decryption_share_contrib, rng);

        Ok(KeyGenAck {
            ack_session_hash: session_hash,
            keygen_id: phase2.keygen_id,
        })
    }

    pub fn sign_ack(
        &mut self,
        phase: SignPhase1,
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
    ) -> Result<Vec<DeviceSend>, ActionError> {
        let SignPhase1 {
            group_sign_req:
                GroupSignReq {
                    parties,
                    agg_nonces,
                    sign_task,
                    access_structure_id,
                },
            device_sign_req:
                DeviceSignReq {
                    nonces: coord_nonce_state,
                    rootkey,
                    coord_share_decryption_contrib,
                },
            encrypted_secret_share,
            session_id,
        } = phase;

        let sign_items = sign_task.sign_items();
        let key_id = KeyId::from_rootkey(rootkey);
        let my_party_index = encrypted_secret_share.share_image.share_index;
        let access_structure_ref = AccessStructureRef {
            key_id,
            access_structure_id,
        };
        let symmetric_key = symm_keygen.get_share_encryption_key(
            access_structure_ref,
            my_party_index,
            coord_share_decryption_contrib,
        );

        let secret_share = encrypted_secret_share
            .ciphertext
            .decrypt(symmetric_key)
            .ok_or_else(|| {
                ActionError::StateInconsistent("couldn't decrypt secret share".into())
            })?;
        let root_paired_secret_share = Xpub::from_rootkey(PairedSecretShare::new_unchecked(
            SecretShare {
                index: my_party_index,
                share: secret_share,
            },
            rootkey,
        ));
        let app_paired_secret_share = root_paired_secret_share.rootkey_to_master_appkey();

        let frost = frost::new_without_nonce_generation::<sha2::Sha256>();

        let sign_sessions = sign_items
            .iter()
            .enumerate()
            .map(|(signature_index, sign_item)| {
                let derived_xonly_key = sign_item
                    .app_tweak
                    .derive_xonly_key(&app_paired_secret_share);
                let message = sign_item.schnorr_fun_message();
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
            .sign_guaranteeing_nonces_destroyed(session_id, coord_nonce_state, sign_sessions)
            .expect("nonce stream already checked to exist");

        Ok(vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::SignatureShare {
                session_id,
                signature_shares,
                replenish_nonces,
            },
        ))])
    }

    pub fn display_backup_ack(
        &mut self,
        phase: BackupDisplayPhase,
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
    ) -> Result<Vec<DeviceSend>, ActionError> {
        let key_data = self
            .keys
            .get(&phase.access_structure_ref.key_id)
            .expect("key must exist");
        let encryption_key = symm_keygen.get_share_encryption_key(
            phase.access_structure_ref,
            phase.party_index,
            phase.coord_share_decryption_contrib,
        );
        let secret_share = phase.encrypted_secret_share.decrypt(encryption_key).ok_or(
            ActionError::StateInconsistent("could not decrypt secret share".into()),
        )?;
        let backup = SecretShare {
            index: phase.party_index,
            share: secret_share,
        }
        .to_bech32_backup();
        Ok(vec![DeviceSend::ToUser(Box::new(
            DeviceToUserMessage::DisplayBackup {
                key_name: key_data.key_name.clone(),
                backup,
            },
        ))])
    }

    pub fn save_complete_share(
        &mut self,
        complete_share: CompleteSecretShare,
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
        coord_contrib: CoordShareDecryptionContrib,
        rng: &mut impl rand_core::RngCore,
    ) {
        let CompleteSecretShare {
            access_structure_ref,
            key_name,
            purpose,
            threshold,
            secret_share,
        } = complete_share;

        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;

        let encrypted_secret_share = EncryptedSecretShare::encrypt(
            secret_share,
            access_structure_ref,
            coord_contrib,
            symm_keygen,
            rng,
        );

        self.mutate(Mutation::NewKey {
            key_id,
            key_name,
            purpose,
        });
        self.mutate(Mutation::NewAccessStructure {
            key_id,
            access_structure_id,
            threshold,
            kind: AccessStructureKind::Master,
        });
        self.mutate(Mutation::SaveShare(Box::new(SaveShareMutation {
            key_id,
            access_structure_id,
            encrypted_secret_share,
        })));
    }

    pub fn tell_coordinator_about_backup_load_result(
        &mut self,
        phase: LoadKnownBackupPhase,
        secret_share: SecretShare,
    ) -> impl IntoIterator<Item = DeviceSend> {
        let share_index = phase.share_image.share_index;
        let access_structure_ref = phase.access_structure_ref;
        let ok = match phase.check_entered_backup(secret_share) {
            Some(complete_share) => {
                self.tmp_loaded_backups.push(complete_share);
                true
            }
            None => false,
        };

        core::iter::once(DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::LoadKnownBackupResult {
                access_structure_ref,
                share_index,
                success: ok,
            },
        )))
    }

    pub fn held_shares(&self) -> Vec<HeldShare> {
        let mut held_shares = vec![];

        for (key_id, key_data) in &self.keys {
            for (access_structure_id, access_structure) in &key_data.access_structures {
                for share in access_structure.shares.values() {
                    if access_structure.kind == AccessStructureKind::Master {
                        held_shares.push(HeldShare {
                            key_name: key_data.key_name.clone(),
                            share_image: share.share_image,
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

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct SaveShareMutation {
    pub key_id: KeyId,
    pub access_structure_id: AccessStructureId,
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
        access_structure_ref: AccessStructureRef,
        party_index: PartyIndex,
        coord_key: CoordShareDecryptionContrib,
    ) -> SymmetricKey;
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CompleteSecretShare {
    pub access_structure_ref: AccessStructureRef,
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub threshold: u16,
    pub secret_share: SecretShare,
}

impl LoadKnownBackup {
    pub fn check_entered_backup(self, share_backup: SecretShare) -> Option<CompleteSecretShare> {
        // could return self in Err but didn't because clippy.
        let share_index = self.share_image.share_index;
        let access_structure_ref = self.access_structure_ref;
        let ok = share_backup.index == share_index
            && g!(share_backup.share * G) == self.share_image.point;

        if ok {
            Some(CompleteSecretShare {
                access_structure_ref,
                key_name: self.key_name,
                purpose: self.purpose,
                threshold: self.threshold,
                secret_share: share_backup,
            })
        } else {
            None
        }
    }
}
