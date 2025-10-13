use crate::device_nonces::{self, AbSlots, MemoryNonceSlot, NonceStreamSlot};
use crate::nonce_stream::CoordNonceStreamState;
use crate::symmetric_encryption::{Ciphertext, SymmetricKey};
use crate::tweak::{self, Xpub};
use crate::{
    bitcoin_transaction, message::*, AccessStructureId, AccessStructureKind, AccessStructureRef,
    ActionError, CheckedSignTask, CoordShareDecryptionContrib, Error, KeyId, KeygenId, Kind,
    MessageResult, RestorationId, SessionHash, ShareImage,
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
use schnorr_fun::frost::chilldkg::certpedpop::{self};
use schnorr_fun::frost::{Fingerprint, PairedSecretShare, SecretShare, ShareIndex, SharedKey};

pub mod keys;
use schnorr_fun::fun::KeyPair;
use schnorr_fun::{frost, fun::prelude::*};
use sha2::Sha256;
mod device_to_user;
pub mod restoration;
pub use device_to_user::*;

/// The number of nonces the device will give out at a time.
pub const NONCE_BATCH_SIZE: u32 = 30;

#[derive(Clone, Debug, PartialEq)]
pub struct FrostSigner<S = MemoryNonceSlot> {
    keypair: KeyPair,
    keys: BTreeMap<KeyId, KeyData>,
    nonce_slots: device_nonces::AbSlots<S>,
    mutations: VecDeque<Mutation>,
    tmp_keygen_phase1: BTreeMap<KeygenId, KeyGenPhase1>,
    tmp_keygen_phase2: BTreeMap<KeygenId, KeyGenPhase2>,
    tmp_keygen_pending_finalize: BTreeMap<KeygenId, (SessionHash, KeyGenPhase4)>,
    restoration: restoration::State,
    pub keygen_fingerprint: Fingerprint,
    pub nonce_batch_size: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyData {
    access_structures: BTreeMap<AccessStructureId, AccessStructureData>,
    purpose: KeyPurpose,
    key_name: String,
    /// Do we know that the `KeyId` is genuinely the one associated with the secret shares we have?
    /// This point is subjective but this device is meant to be able to
    verified: bool,
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
    pub shares: BTreeMap<ShareIndex, EncryptedSecretShare>,
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
        symm_keygen: &mut impl DeviceSecretDerivation,
        rng: &mut impl rand_core::RngCore,
    ) -> Self {
        let share_image = secret_share.share_image();
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
        symm_keygen: &mut impl DeviceSecretDerivation,
    ) -> Option<SecretShare> {
        let encryption_key = symm_keygen.get_share_encryption_key(
            access_structure_ref,
            self.share_image.index,
            coord_contrib,
        );

        self.ciphertext
            .decrypt(encryption_key)
            .map(|share| SecretShare {
                index: self.share_image.index,
                share,
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenPhase1 {
    pub device_to_share_index: BTreeMap<DeviceId, NonZeroU32>,
    pub input_state: certpedpop::Contributor,
    pub threshold: u16,
    pub key_name: String,
    pub key_purpose: KeyPurpose,
    pub coordinator_public_key: Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenPhase2 {
    pub keygen_id: KeygenId,
    pub device_to_share_index: BTreeMap<DeviceId, NonZeroU32>,
    paired_secret_share: PairedSecretShare,
    agg_input: certpedpop::AggKeygenInput,
    key_name: String,
    key_purpose: KeyPurpose,
    pub coordinator_public_key: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyGenPhase3 {
    pub keygen_id: KeygenId,
    session_hash: SessionHash,
    key_name: String,
    key_purpose: KeyPurpose,
    t_of_n: (u16, u16),
    shared_key: SharedKey,
    secret_share: PairedSecretShare,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyGenPhase4 {
    key_name: String,
    key_purpose: KeyPurpose,
    access_structure_ref: AccessStructureRef,
    access_structure_kind: AccessStructureKind,
    encrypted_secret_share: EncryptedSecretShare,
    threshold: u16,
}

impl KeyGenPhase2 {
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

impl KeyGenPhase3 {
    pub fn key_name(&self) -> &str {
        self.key_name.as_str()
    }
    pub fn t_of_n(&self) -> (u16, u16) {
        self.t_of_n
    }
    pub fn session_hash(&self) -> SessionHash {
        self.session_hash
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SignPhase1 {
    group_sign_req: GroupSignReq<CheckedSignTask>,
    device_sign_req: DeviceSignReq,
    encrypted_secret_share: EncryptedSecretShare,
    pub session_id: SignSessionId,
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
            tmp_keygen_phase1: Default::default(),
            tmp_keygen_phase2: Default::default(),
            tmp_keygen_pending_finalize: Default::default(),
            restoration: Default::default(),
            keygen_fingerprint: Fingerprint::FROST_V0,
            nonce_batch_size: NONCE_BATCH_SIZE,
        }
    }

    pub fn mutate(&mut self, mutation: Mutation) {
        if let Some(mutation) = self.apply_mutation(mutation) {
            self.mutations.push_back(mutation);
        }
    }

    pub fn apply_mutation(&mut self, mutation: Mutation) -> Option<Mutation> {
        use Mutation::*;
        match mutation {
            Keygen(keys::KeyMutation::NewKey {
                key_id,
                ref key_name,
                purpose,
            }) => {
                self.keys.insert(
                    key_id,
                    KeyData {
                        purpose,
                        access_structures: Default::default(),
                        key_name: key_name.into(),
                        verified: false,
                    },
                );
            }
            Keygen(keys::KeyMutation::NewAccessStructure {
                access_structure_ref,
                kind,
                threshold,
            }) => {
                self.keys
                    .entry(access_structure_ref.key_id)
                    .and_modify(|key_data| {
                        key_data.access_structures.insert(
                            access_structure_ref.access_structure_id,
                            AccessStructureData {
                                kind,
                                threshold,
                                shares: Default::default(),
                            },
                        );
                    });
            }
            Keygen(keys::KeyMutation::SaveShare(ref boxed)) => {
                let SaveShareMutation {
                    access_structure_ref,
                    encrypted_secret_share,
                } = boxed.as_ref();
                self.keys
                    .entry(access_structure_ref.key_id)
                    .and_modify(|key_data| {
                        key_data
                            .access_structures
                            .entry(access_structure_ref.access_structure_id)
                            .and_modify(|access_structure_data| {
                                access_structure_data.shares.insert(
                                    encrypted_secret_share.share_image.index,
                                    *encrypted_secret_share,
                                );
                            });
                    });
            }
            Restoration(restoration_mutation) => {
                return self
                    .restoration
                    .apply_mutation_restoration(restoration_mutation)
                    .map(Mutation::Restoration);
            }
        }

        Some(mutation)
    }

    pub fn staged_mutations(&mut self) -> &mut VecDeque<Mutation> {
        &mut self.mutations
    }

    pub fn clear_unfinished_keygens(&mut self) {
        self.tmp_keygen_phase1.clear();
        self.tmp_keygen_phase2.clear();
        self.tmp_keygen_pending_finalize.clear();
    }

    pub fn clear_tmp_data(&mut self) {
        self.clear_unfinished_keygens();
        self.restoration.clear_tmp_data();
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
            Signing(signing::CoordinatorSigning::OpenNonceStreams(open_nonce_stream)) => {
                let mut tasks = vec![];
                // we need to order prioritize streams that already exist since not getting a
                // response to this message the coordinator will think that everything is ok.
                let (existing, new): (Vec<_>, Vec<_>) = open_nonce_stream
                    .streams
                    .iter()
                    .partition(|stream| self.nonce_slots.get(stream.stream_id).is_some());
                let ordered_streams = existing
                    .into_iter()
                    .chain::<Vec<CoordNonceStreamState>>(new)
                    // If we take more than the total available we risk overwriting slots
                    .take(self.nonce_slots.total_slots());
                for coord_stream_state in ordered_streams {
                    let slot = self
                        .nonce_slots
                        .get_or_create(coord_stream_state.stream_id, rng);
                    if let Some(task) = slot.reconcile_coord_nonce_stream_state(
                        coord_stream_state,
                        self.nonce_batch_size,
                    ) {
                        tasks.push(task);
                    }
                }

                // If there are no tasks, send empty response immediately
                // Otherwise, send tasks for async processing
                if tasks.is_empty() {
                    Ok(vec![DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::Signing(
                            signing::DeviceSigning::NonceResponse { segments: vec![] },
                        ),
                    ))])
                } else {
                    Ok(vec![DeviceSend::ToUser(Box::new(
                        DeviceToUserMessage::NonceJobs(device_nonces::NonceJobBatch::new(tasks)),
                    ))])
                }
            }
            KeyGen(keygen_msg) => match keygen_msg {
                self::Keygen::Begin(begin) => {
                    let device_to_share_index = begin.device_to_share_index();
                    if !device_to_share_index.contains_key(&self.device_id()) {
                        return Ok(vec![]);
                    }
                    let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();

                    let share_receivers_enckeys = device_to_share_index
                        .iter()
                        .map(|(device, share_index)| {
                            (ShareIndex::from(*share_index), device.pubkey())
                        })
                        .collect::<BTreeMap<_, _>>();
                    let my_index =
                        device_to_share_index
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

                    let (input_state, keygen_input) = certpedpop::Contributor::gen_keygen_input(
                        &schnorr,
                        begin.threshold as u32,
                        &share_receivers_enckeys,
                        (*my_index).into(),
                        rng,
                    );
                    self.tmp_keygen_phase1.insert(
                        begin.keygen_id,
                        KeyGenPhase1 {
                            device_to_share_index,
                            input_state,
                            threshold: begin.threshold,
                            key_name: begin.key_name.clone(),
                            key_purpose: begin.purpose,
                            coordinator_public_key: begin.coordinator_public_key,
                        },
                    );
                    Ok(vec![DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::KeyGen(keygen::DeviceKeygen::Response(
                            KeyGenResponse {
                                keygen_id: begin.keygen_id,
                                input: Box::new(keygen_input),
                            },
                        )),
                    ))])
                }
                self::Keygen::CertifyPlease {
                    keygen_id,
                    agg_input,
                } => {
                    let cert_scheme = certpedpop::vrf_cert::VrfCertScheme::<Sha256>::new(
                        crate::message::keygen::VRF_CERT_SCHEME_ID,
                    );
                    let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
                    let phase1 = self.tmp_keygen_phase1.remove(&keygen_id).ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "no keygen state for provided keygen_id",
                        )
                    })?;

                    let my_index = phase1
                        .device_to_share_index
                        .get(&self.device_id())
                        .expect("already checked");

                    //XXX: We check the fingerprint so that a (mildly) malicious
                    // coordinator cannot create key generations without the
                    // fingerprint.
                    if !agg_input
                        .shared_key()
                        .check_fingerprint::<sha2::Sha256>(self.keygen_fingerprint)
                    {
                        return Err(Error::signer_invalid_message(
                            &message,
                            "key generation did not match the fingerprint",
                        ));
                    }

                    let (paired_secret_share, vrf_cert) = phase1
                        .input_state
                        .verify_receive_share_and_certify(
                            &schnorr,
                            &cert_scheme,
                            (*my_index).into(),
                            self.keypair(),
                            &agg_input,
                        )
                        .map_err(|e| {
                            Error::signer_invalid_message(
                                &message,
                                format!("Failed to verify and receive share: {e}"),
                            )
                        })?;

                    let paired_secret_share = paired_secret_share.non_zero().ok_or_else(|| {
                        Error::signer_invalid_message(&message, "keygen produced a zero shared key")
                    })?;

                    self.tmp_keygen_phase2.insert(
                        keygen_id,
                        KeyGenPhase2 {
                            keygen_id,
                            device_to_share_index: phase1.device_to_share_index,
                            paired_secret_share,
                            agg_input,
                            key_name: phase1.key_name.clone(),
                            key_purpose: phase1.key_purpose,
                            coordinator_public_key: phase1.coordinator_public_key,
                        },
                    );
                    Ok(vec![DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::KeyGen(keygen::DeviceKeygen::Certify {
                            keygen_id,
                            vrf_cert,
                        }),
                    ))])
                }
                self::Keygen::Check {
                    keygen_id,
                    certificate,
                } => {
                    let phase2 = self.tmp_keygen_phase2.remove(&keygen_id).ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "no keygen state for provided keygen_id",
                        )
                    })?;

                    // Reconstruct the certifier with all contributor public keys
                    let cert_scheme = certpedpop::vrf_cert::VrfCertScheme::<Sha256>::new(
                        crate::message::keygen::VRF_CERT_SCHEME_ID,
                    );

                    let mut certifier = certpedpop::Certifier::new(
                        cert_scheme,
                        phase2.agg_input.clone(),
                        &[phase2.coordinator_public_key],
                    );

                    // Add all certificates to the certifier
                    for (pubkey, cert) in certificate {
                        certifier.receive_certificate(pubkey, cert).map_err(|e| {
                            Error::signer_invalid_message(
                                &message,
                                format!("Invalid certificate received: {e}"),
                            )
                        })?;
                    }

                    // Verify we have all certificates and create the certified keygen
                    let certified_keygen = certifier.finish().map_err(|e| {
                        Error::signer_invalid_message(
                            &message,
                            format!("Missing certificates or verification failed: {e}"),
                        )
                    })?;

                    let session_hash = SessionHash::from_certified_keygen(&certified_keygen);

                    let phase3 = KeyGenPhase3 {
                        keygen_id,
                        t_of_n: phase2.t_of_n(),
                        key_name: phase2.key_name,
                        key_purpose: phase2.key_purpose,
                        shared_key: certified_keygen
                            .agg_input()
                            .shared_key()
                            .non_zero()
                            .expect("we contributed to coefficient -- can't be zero"),
                        secret_share: phase2.paired_secret_share,
                        session_hash,
                    };

                    Ok(vec![DeviceSend::ToUser(Box::new(
                        DeviceToUserMessage::CheckKeyGen {
                            phase: Box::new(phase3),
                        },
                    ))])
                }
                self::Keygen::Finalize { keygen_id } => {
                    let (_session_hash, keygen_pending_finalize) = self
                        .tmp_keygen_pending_finalize
                        .remove(&keygen_id)
                        .ok_or(Error::signer_invalid_message(
                            &message,
                            format!("device doesn't have keygen for {keygen_id}"),
                        ))?;
                    let key_name = keygen_pending_finalize.key_name.clone();
                    self.save_complete_share(keygen_pending_finalize);

                    Ok(vec![DeviceSend::ToUser(Box::new(
                        DeviceToUserMessage::FinalizeKeyGen { key_name },
                    ))])
                }
            },
            Signing(signing::CoordinatorSigning::RequestSign(request_sign)) => {
                let self::RequestSign {
                    group_sign_req,
                    device_sign_req,
                } = *request_sign;
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

                // Just verify the nonce stream exists but don't check availability
                // The signing logic will handle cached signatures naturally
                let _nonce_slot = self
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

                // Removed are_nonces_available check - let the signing system handle it
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
            ScreenVerify(screen_verify::ScreenVerify::VerifyAddress {
                master_appkey,
                derivation_index,
            }) => {
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
            Restoration(message) => self.recv_restoration_message(message, rng),
        }
    }

    pub fn keygen_ack(
        &mut self,
        phase: KeyGenPhase3,
        symm_key_gen: &mut impl DeviceSecretDerivation,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<KeyGenAck, ActionError> {
        let secret_share = phase.secret_share;
        let key_name = phase.key_name;
        let rootkey = secret_share.public_key();
        let key_id = KeyId::from_rootkey(rootkey);
        let root_shared_key = Xpub::from_rootkey(phase.shared_key);
        let app_shared_key = root_shared_key.rootkey_to_master_appkey();
        let access_structure_id =
            AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());

        // SHARE ENCRYPTION NOTE 1: We make the device gnerate the encryption key for the share right after keygen rather
        // than letting the coordinator send it to the device to protect against malicious
        // coordinators. A coordinator could provide garbage for example and then the device would
        // never be able to decrypt its share again.
        let decryption_share_contrib = CoordShareDecryptionContrib::for_master_share(
            self.device_id(),
            secret_share.index(),
            &root_shared_key.key,
        );

        let threshold = app_shared_key
            .key
            .threshold()
            .try_into()
            .expect("threshold was too large");

        let access_structure_ref = AccessStructureRef {
            key_id,
            access_structure_id,
        };

        let encrypted_secret_share = EncryptedSecretShare::encrypt(
            *secret_share.secret_share(),
            access_structure_ref,
            decryption_share_contrib,
            symm_key_gen,
            rng,
        );

        self.tmp_keygen_pending_finalize.insert(
            phase.keygen_id,
            (
                phase.session_hash,
                KeyGenPhase4 {
                    key_name,
                    key_purpose: phase.key_purpose,
                    access_structure_ref,
                    access_structure_kind: AccessStructureKind::Master,
                    threshold,
                    encrypted_secret_share,
                },
            ),
        );

        Ok(KeyGenAck {
            ack_session_hash: phase.session_hash,
            keygen_id: phase.keygen_id,
        })
    }

    pub fn sign_ack(
        &mut self,
        phase: SignPhase1,
        symm_keygen: &mut impl DeviceSecretDerivation,
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
        let my_party_index = encrypted_secret_share.share_image.index;
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

        let (signature_shares, replenish_task) = self
            .nonce_slots
            .sign_guaranteeing_nonces_destroyed(
                session_id,
                coord_nonce_state,
                sign_sessions,
                symm_keygen,
                self.nonce_batch_size,
            )
            .map_err(|e| ActionError::StateInconsistent(e.to_string()))?;

        // Run the replenishment task synchronously if present
        let replenish_nonces = if let Some(mut task) = replenish_task {
            task.run_until_finished(symm_keygen);
            Some(task.into_segment())
        } else {
            None
        };

        Ok(vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::Signing(signing::DeviceSigning::SignatureShare {
                session_id,
                signature_shares,
                replenish_nonces,
            }),
        ))])
    }

    fn save_complete_share(&mut self, phase: KeyGenPhase4) {
        self.mutate(Mutation::Keygen(keys::KeyMutation::NewKey {
            key_id: phase.access_structure_ref.key_id,
            key_name: phase.key_name,
            purpose: phase.key_purpose,
        }));
        self.mutate(Mutation::Keygen(keys::KeyMutation::NewAccessStructure {
            access_structure_ref: phase.access_structure_ref,
            threshold: phase.threshold,
            kind: phase.access_structure_kind,
        }));
        self.mutate(Mutation::Keygen(keys::KeyMutation::SaveShare(Box::new(
            SaveShareMutation {
                access_structure_ref: phase.access_structure_ref,
                encrypted_secret_share: phase.encrypted_secret_share,
            },
        ))));
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

    pub fn get_encrypted_share(
        &self,
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    ) -> Option<EncryptedSecretShare> {
        self.keys
            .get(&access_structure_ref.key_id)?
            .access_structures
            .get(&access_structure_ref.access_structure_id)?
            .shares
            .get(&share_index)
            .cloned()
    }
}

impl FrostSigner<MemoryNonceSlot> {
    /// For testing only
    pub fn new_random(rng: &mut impl rand_core::RngCore, nonce_streams: usize) -> Self {
        Self::new_random_with_nonce_batch_size(rng, nonce_streams, NONCE_BATCH_SIZE)
    }

    /// For testing only - with configurable nonce_batch_size
    pub fn new_random_with_nonce_batch_size(
        rng: &mut impl rand_core::RngCore,
        nonce_streams: usize,
        nonce_batch_size: u32,
    ) -> Self {
        let mut signer = Self::new(
            KeyPair::<Normal>::new(Scalar::random(rng)),
            AbSlots::new(
                (0..nonce_streams)
                    .map(|_| MemoryNonceSlot::default())
                    .collect(),
            ),
        );
        signer.nonce_batch_size = nonce_batch_size;
        signer
    }
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct SaveShareMutation {
    pub access_structure_ref: AccessStructureRef,
    pub encrypted_secret_share: EncryptedSecretShare,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, frostsnap_macros::Kind)]
pub enum Mutation {
    #[delegate_kind]
    Keygen(keys::KeyMutation),
    #[delegate_kind]
    Restoration(restoration::RestorationMutation),
}

pub trait DeviceSecretDerivation {
    fn get_share_encryption_key(
        &mut self,
        access_structure_ref: AccessStructureRef,
        party_index: ShareIndex,
        coord_key: CoordShareDecryptionContrib,
    ) -> SymmetricKey;

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: crate::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32];
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CompleteSecretShare {
    pub access_structure_ref: AccessStructureRef,
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub threshold: u16,
    pub secret_share: SecretShare,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct IncompleteSecretShare {
    pub secret_share: SecretShare,
    pub restoration_id: RestorationId,
}
