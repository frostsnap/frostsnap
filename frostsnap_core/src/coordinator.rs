use crate::{
    device::{KeyPurpose, NONCE_BATCH_SIZE},
    map_ext::*,
    message::*,
    nonce_stream::NonceStreamId,
    symmetric_encryption::{Ciphertext, SymmetricKey},
    tweak::Xpub,
    AccessStructureId, AccessStructureKind, AccessStructureRef, ActionError,
    CoordShareDecryptionContrib, DeviceId, Error, Gist, KeyId, KeygenId, Kind, MasterAppkey,
    MessageResult, RestorationId, SessionHash, ShareImage, SignSessionId, WireSignTask,
};
use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::String,
    vec::Vec,
};
use core::fmt;
use frostsnap_macros::Kind;
use schnorr_fun::{
    frost::{chilldkg::certpedpop, ShareIndex, SharedKey},
    fun::{prelude::*, KeyPair},
};
use sha2::Sha256;
use std::collections::HashMap;
use tracing::{event, Level};

mod coordinator_to_user;
pub mod keys;
pub mod remote_keygen;
pub mod remote_signing;
pub mod restoration;
pub mod signing;
pub use coordinator_to_user::*;
pub use keys::BeginKeygen;
pub use signing::*;

pub const MIN_NONCES_BEFORE_REQUEST: u32 = NONCE_BATCH_SIZE / 2;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrostCoordinator {
    keys: BTreeMap<KeyId, CoordFrostKey>,
    key_order: Vec<KeyId>,
    pending_keygens: HashMap<KeygenId, KeyGenState>,
    mutations: VecDeque<Mutation>,
    signing: signing::State,
    remote_signing: remote_signing::State,
    restoration: restoration::State,
    remote_keygen: remote_keygen::State,
    pub keygen_fingerprint: schnorr_fun::frost::Fingerprint,
}

/// The key data needed for signing: the shared key and its purpose.
#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct KeyContext {
    pub app_shared_key: Xpub<SharedKey>,
    pub purpose: KeyPurpose,
}

impl KeyContext {
    pub fn master_appkey(&self) -> MasterAppkey {
        MasterAppkey::from_xpub_unchecked(&self.app_shared_key)
    }

    pub fn key_id(&self) -> KeyId {
        self.master_appkey().key_id()
    }

    pub fn access_structure_id(&self) -> AccessStructureId {
        AccessStructureId::from_app_poly(self.app_shared_key.key.point_polynomial())
    }

    pub fn access_structure_ref(&self) -> AccessStructureRef {
        AccessStructureRef {
            key_id: self.key_id(),
            access_structure_id: self.access_structure_id(),
        }
    }

    pub fn threshold(&self) -> usize {
        self.app_shared_key.key.threshold()
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CoordFrostKey {
    pub key_id: KeyId,
    pub complete_key: CompleteKey,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CompleteKey {
    pub master_appkey: MasterAppkey,
    pub encrypted_rootkey: Ciphertext<33, Point>,
    pub access_structures: HashMap<AccessStructureId, CoordAccessStructure>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShareLocation {
    pub device_ids: Vec<DeviceId>,
    pub share_index: ShareIndex,
    pub key_name: String,
    pub key_purpose: KeyPurpose,
    pub key_state: KeyLocationState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyLocationState {
    Complete {
        access_structure_ref: AccessStructureRef,
    },
    Restoring {
        restoration_id: RestorationId,
    },
}

impl CompleteKey {
    pub fn coord_share_decryption_contrib(
        &self,
        access_structure_id: AccessStructureId,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> Option<(Point, CoordShareDecryptionContrib)> {
        let root_shared_key = self.root_shared_key(access_structure_id, encryption_key)?;
        let share_index = *self
            .access_structures
            .get(&access_structure_id)?
            .device_to_share_index
            .get(&device_id)?;
        Some((
            root_shared_key.public_key(),
            CoordShareDecryptionContrib::for_master_share(device_id, share_index, &root_shared_key),
        ))
    }

    pub fn root_shared_key(
        &self,
        access_structure_id: AccessStructureId,
        encryption_key: SymmetricKey,
    ) -> Option<SharedKey> {
        let access_structure = self.access_structures.get(&access_structure_id)?;
        let rootkey = self.encrypted_rootkey.decrypt(encryption_key)?;
        let mut poly = access_structure
            .app_shared_key
            .key
            .point_polynomial()
            .to_vec();
        poly[0] = rootkey.mark_zero();
        debug_assert!(
            MasterAppkey::derive_from_rootkey(rootkey) == access_structure.master_appkey()
        );
        Some(SharedKey::from_poly(poly).non_zero().expect("invariant"))
    }
}

#[cfg(feature = "coordinator")]
#[macro_export]
macro_rules! fail {
    ($($fail:tt)*) => {{
        tracing::event!(
            tracing::Level::ERROR,
            $($fail)*
        );
        debug_assert!(false, $($fail)*);
        return None;
    }};
}

impl CoordFrostKey {
    pub fn get_access_structure(
        &self,
        access_structure_id: AccessStructureId,
    ) -> Option<CoordAccessStructure> {
        self.complete_key
            .access_structures
            .get(&access_structure_id)
            .cloned()
    }

    pub fn access_structures(&self) -> impl Iterator<Item = CoordAccessStructure> + '_ {
        self.complete_key.access_structures.values().cloned()
    }

    pub fn master_access_structure(&self) -> CoordAccessStructure {
        self.access_structures().next().unwrap()
    }
}

impl FrostCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mutate(&mut self, mutation: Mutation) {
        let kind = mutation.kind();
        if let Some(reduced_mutation) = self.apply_mutation(mutation) {
            event!(Level::DEBUG, gist = reduced_mutation.gist(), "mutating");
            self.mutations.push_back(reduced_mutation);
        } else {
            event!(Level::DEBUG, kind = kind, "ignoring mutation");
        }
    }

    pub fn apply_mutation(&mut self, mutation: Mutation) -> Option<Mutation> {
        fn ensure_key<'a>(
            coord: &'a mut FrostCoordinator,
            complete_key: self::CompleteKey,
            key_name: &str,
            purpose: KeyPurpose,
        ) -> &'a mut CoordFrostKey {
            let key_id = complete_key.master_appkey.key_id();
            let exists = coord.keys.contains_key(&key_id);
            let key = coord.keys.entry(key_id).or_insert_with(|| CoordFrostKey {
                key_id,
                complete_key,
                key_name: key_name.to_owned(),
                purpose,
            });
            if !exists {
                coord.key_order.push(key_id);
            }
            key
        }
        use Mutation::*;
        match mutation {
            Keygen(keys::KeyMutation::NewKey {
                ref complete_key,
                ref key_name,
                purpose,
            }) => {
                ensure_key(self, complete_key.clone(), key_name, purpose);
            }
            Keygen(keys::KeyMutation::NewAccessStructure {
                ref shared_key,
                kind,
            }) => {
                let access_structure_id =
                    AccessStructureId::from_app_poly(shared_key.key().point_polynomial());
                let appkey = MasterAppkey::from_xpub_unchecked(shared_key);
                let key_id = appkey.key_id();

                match self.keys.get_mut(&key_id) {
                    Some(key_data) => {
                        let exists = key_data
                            .complete_key
                            .access_structures
                            .contains_key(&access_structure_id);

                        if exists {
                            return None;
                        }

                        key_data.complete_key.access_structures.insert(
                            access_structure_id,
                            CoordAccessStructure {
                                app_shared_key: shared_key.clone(),
                                device_to_share_index: Default::default(),
                                kind,
                            },
                        );
                    }
                    None => {
                        fail!("access structure added to non-existent key");
                    }
                }
            }
            Keygen(keys::KeyMutation::NewShare {
                access_structure_ref,
                device_id,
                share_index,
            }) => match self.keys.get_mut(&access_structure_ref.key_id) {
                Some(key_data) => {
                    let complete_key = &mut key_data.complete_key;

                    match complete_key
                        .access_structures
                        .get_mut(&access_structure_ref.access_structure_id)
                    {
                        Some(access_structure) => {
                            let changed = access_structure
                                .device_to_share_index
                                .insert(device_id, share_index)
                                != Some(share_index);

                            if !changed {
                                return None;
                            }
                        }
                        None => {
                            fail!(
                                "share added to non-existent access structure {:?}",
                                access_structure_ref
                            );
                        }
                    }
                }
                None => {
                    fail!(
                        "share added to non-existent key: {}",
                        access_structure_ref.key_id
                    );
                }
            },
            Keygen(keys::KeyMutation::DeleteShare {
                access_structure_ref,
                device_id,
            }) => match self.keys.get_mut(&access_structure_ref.key_id) {
                Some(key_data) => {
                    match key_data
                        .complete_key
                        .access_structures
                        .get_mut(&access_structure_ref.access_structure_id)
                    {
                        Some(access_structure) => {
                            access_structure.device_to_share_index.remove(&device_id)?;
                        }
                        None => {
                            fail!(
                                "share deleted from non-existent access structure {:?}",
                                access_structure_ref
                            );
                        }
                    }
                }
                None => {
                    fail!(
                        "share deleted from non-existent key: {}",
                        access_structure_ref.key_id
                    );
                }
            },
            Keygen(keys::KeyMutation::DeleteKey(key_id)) => {
                self.keys.remove(&key_id)?;
                self.key_order.retain(|&entry| entry != key_id);
                self.restoration.clear_up_key_deletion(key_id);
                self.signing.clear_up_key_deletion(key_id);
                self.remote_signing
                    .clear_up_key_deletion(key_id, &mut self.signing.nonce_cache);
            }
            Signing(inner) => {
                return self
                    .signing
                    .apply_mutation_signing(inner)
                    .map(Mutation::Signing);
            }
            RemoteSigning(inner) => {
                return self
                    .remote_signing
                    .apply_mutation(inner, &mut self.signing.nonce_cache)
                    .map(Mutation::RemoteSigning);
            }
            Restoration(inner) => {
                return self
                    .restoration
                    .apply_mutation_restoration(inner, self.keygen_fingerprint)
                    .map(Mutation::Restoration);
            }
        }

        Some(mutation)
    }

    pub fn take_staged_mutations(&mut self) -> VecDeque<Mutation> {
        core::mem::take(&mut self.mutations)
    }

    pub fn staged_mutations(&self) -> &VecDeque<Mutation> {
        &self.mutations
    }

    pub fn iter_keys(&self) -> impl Iterator<Item = &CoordFrostKey> + '_ {
        self.key_order
            .iter()
            .map(|key_id| self.keys.get(key_id).expect("invariant"))
    }

    pub fn iter_access_structures(&self) -> impl Iterator<Item = CoordAccessStructure> + '_ {
        self.keys
            .values()
            .flat_map(|key_data| key_data.access_structures())
    }

    pub fn iter_shares(
        &self,
        encryption_key: SymmetricKey,
    ) -> impl Iterator<Item = (ShareImage, ShareLocation)> + '_ {
        let complete_wallet_shares = self
            .iter_access_structures()
            .filter_map(move |access_structure| {
                let access_structure_ref = access_structure.access_structure_ref();
                self.root_shared_key(access_structure_ref, encryption_key)
                    .map(|root_shared_key| {
                        (access_structure, access_structure_ref, root_shared_key)
                    })
            })
            .flat_map(
                move |(access_structure, access_structure_ref, root_shared_key)| {
                    let key = self
                        .keys
                        .get(&access_structure_ref.key_id)
                        .expect("must exist");
                    let key_name = key.key_name.clone();
                    let key_purpose = key.purpose;

                    access_structure.share_index_to_devices().into_iter().map(
                        move |(share_index, device_ids)| {
                            let share_image = root_shared_key.share_image(share_index);
                            (
                                share_image,
                                ShareLocation {
                                    device_ids,
                                    share_index,
                                    key_name: key_name.clone(),
                                    key_purpose,
                                    key_state: KeyLocationState::Complete {
                                        access_structure_ref,
                                    },
                                },
                            )
                        },
                    )
                },
            );

        let restoration_shares = self
            .restoration
            .restorations
            .values()
            .flat_map(|restoration| {
                let restoration_id = restoration.restoration_id;
                let key_name = restoration.key_name.clone();
                let key_purpose = restoration.key_purpose;

                restoration
                    .access_structure
                    .share_image_to_devices()
                    .into_iter()
                    .map(move |(share_image, device_ids)| {
                        (
                            share_image,
                            ShareLocation {
                                device_ids,
                                share_index: share_image.index,
                                key_name: key_name.clone(),
                                key_purpose,
                                key_state: KeyLocationState::Restoring { restoration_id },
                            },
                        )
                    })
            });

        complete_wallet_shares.chain(restoration_shares)
    }

    pub fn find_share(
        &self,
        share_image: ShareImage,
        encryption_key: SymmetricKey,
    ) -> Option<ShareLocation> {
        // Check complete wallets first (they have priority)
        let found = self.iter_access_structures().find(|access_structure| {
            let access_structure_ref = access_structure.access_structure_ref();
            let Some(root_shared_key) = self.root_shared_key(access_structure_ref, encryption_key)
            else {
                return false;
            };

            let computed_share_image = root_shared_key.share_image(share_image.index);
            computed_share_image == share_image
        });

        if let Some(access_structure) = found {
            let access_structure_ref = access_structure.access_structure_ref();
            let device_ids = access_structure
                .share_index_to_devices()
                .get(&share_image.index)
                .cloned()
                .unwrap_or_default();
            let key = self
                .keys
                .get(&access_structure_ref.key_id)
                .expect("must exist");

            return Some(ShareLocation {
                device_ids,
                share_index: share_image.index,
                key_name: key.key_name.clone(),
                key_purpose: key.purpose,
                key_state: KeyLocationState::Complete {
                    access_structure_ref,
                },
            });
        }

        // Check restorations
        for restoration in self.restoration.restorations.values() {
            let share_image_to_devices = restoration.access_structure.share_image_to_devices();

            // Check physical shares
            if let Some(device_ids) = share_image_to_devices.get(&share_image) {
                return Some(ShareLocation {
                    device_ids: device_ids.clone(),
                    share_index: share_image.index,
                    key_name: restoration.key_name.clone(),
                    key_purpose: restoration.key_purpose,
                    key_state: KeyLocationState::Restoring {
                        restoration_id: restoration.restoration_id,
                    },
                });
            }

            // Check virtual shares (via cached SharedKey)
            if let Some(shared_key) = &restoration.access_structure.shared_key {
                let computed_share_image = shared_key.share_image(share_image.index);
                if computed_share_image == share_image {
                    return Some(ShareLocation {
                        device_ids: Vec::new(),
                        share_index: share_image.index,
                        key_name: restoration.key_name.clone(),
                        key_purpose: restoration.key_purpose,
                        key_state: KeyLocationState::Restoring {
                            restoration_id: restoration.restoration_id,
                        },
                    });
                }
            }
        }

        None
    }

    pub fn get_frost_key(&self, key_id: KeyId) -> Option<&CoordFrostKey> {
        self.keys.get(&key_id)
    }

    pub fn recv_device_message(
        &mut self,
        from: DeviceId,
        message: DeviceToCoordinatorMessage,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        let message_kind = message.kind();
        match message {
            DeviceToCoordinatorMessage::Signing(message) => {
                self.recv_signing_message(from, message)
            }
            DeviceToCoordinatorMessage::KeyGen(keygen::DeviceKeygen::Response(response)) => {
                let keygen_id = response.keygen_id;
                if self.is_remote_keygen_active(keygen_id) {
                    return self.receive_device_keygen_response(from, response);
                }
                let (state, entry) = self.pending_keygens.take_entry(keygen_id);

                match state {
                    Some(KeyGenState::WaitingForResponses(mut state)) => {
                        let cert_scheme = certpedpop::vrf_cert::VrfCertScheme::<Sha256>::new(
                            crate::message::keygen::VRF_CERT_SCHEME_ID,
                        );
                        let share_index = state.device_to_share_index.get(&from).ok_or(
                            Error::coordinator_invalid_message(
                                message_kind,
                                "got share from device that was not part of keygen",
                            ),
                        )?;

                        // Input-generator indices: [0..n_coordinators) are coordinators;
                        // [n_coordinators..n_coordinators + n_devices) are devices in
                        // share-index order. Share indices are 1-based, hence the `- 1`.
                        let n_coordinators = state.coordinator_public_keys.len() as u32;
                        let input_gen_index = u32::from(*share_index) - 1 + n_coordinators;

                        state
                            .input_aggregator
                            .add_input(
                                &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                                input_gen_index,
                                *response.input,
                            )
                            .map_err(|e| Error::coordinator_invalid_message(message_kind, e))?;

                        let mut outgoing =
                            vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                                keygen_id,
                                inner: CoordinatorToUserKeyGenMessage::ReceivedShares { from },
                            })];

                        if state.input_aggregator.is_finished() {
                            // Remove the entry to take ownership
                            let mut agg_input = state.input_aggregator.finish().unwrap();
                            agg_input.grind_fingerprint::<Sha256>(self.keygen_fingerprint);

                            // First we calculate our (the coordinator) certificate and add our VRF outputs
                            let sig = state
                                .contributer
                                .verify_agg_input(&cert_scheme, &agg_input, &state.my_keypair)
                                .expect("will be able to certify agg_input we created");

                            let mut certifier = certpedpop::Certifier::new(
                                cert_scheme,
                                agg_input.clone(),
                                &state.coordinator_public_keys,
                            );

                            certifier
                                .receive_certificate(state.my_keypair.public_key(), sig)
                                .expect("will be able to verify our own certificate");

                            outgoing.push(CoordinatorSend::ToDevice {
                                destinations: state.device_to_share_index.keys().cloned().collect(),
                                message: Keygen::CertifyPlease {
                                    keygen_id,
                                    agg_input,
                                }
                                .into(),
                            });

                            entry.insert(KeyGenState::WaitingForCertificates(
                                KeyGenWaitingForCertificates {
                                    keygen_id: state.keygen_id,
                                    device_to_share_index: state.device_to_share_index,
                                    pending_key_name: state.pending_key_name,
                                    purpose: state.purpose,
                                    certifier,
                                    coordinator_keypair: state.my_keypair,
                                },
                            ));
                        } else {
                            entry.insert(KeyGenState::WaitingForResponses(state));
                        }
                        Ok(outgoing)
                    }
                    _ => Err(Error::coordinator_invalid_message(
                        message_kind,
                        "keygen wasn't in WaitingForResponses state",
                    )),
                }
            }
            DeviceToCoordinatorMessage::KeyGen(keygen::DeviceKeygen::Certify {
                keygen_id,
                vrf_cert,
            }) => {
                if self.is_remote_keygen_active(keygen_id) {
                    return self.receive_device_keygen_certify(from, keygen_id, vrf_cert);
                }
                let mut outgoing = vec![];
                let (state, entry) = self.pending_keygens.take_entry(keygen_id);

                match state {
                    Some(KeyGenState::WaitingForCertificates(mut state)) => {
                        // Store device output and its certificate
                        state.certifier
                            .receive_certificate(from.pubkey(), vrf_cert)
                            .map_err(|_| {
                                Error::coordinator_invalid_message(
                                    message_kind,
                                    "Invalid VRF proof received",
                                )
                            })?;

                        // contributers are the devices plus one coordinator
                        if state.certifier.is_finished() {
                            let certified_keygen = state.certifier
                                .finish()
                                .expect("just checked is_finished");

                            let session_hash = SessionHash::from_certified_keygen(&certified_keygen);

                            // Extract certificates from the certified keygen
                            let certificate = certified_keygen
                                .certificate()
                                .iter()
                                .map(|(pk, cert)| (*pk, cert.clone()))
                                .collect();

                            outgoing.push(CoordinatorSend::ToDevice {
                                destinations: state.device_to_share_index.keys().cloned().collect(),
                                message: Keygen::Check {
                                    keygen_id,
                                    certificate,
                                }
                                .into(),
                            });

                            outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                                keygen_id,
                                inner: CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash },
                            }));

                            // Insert new state
                            entry.insert(
                                KeyGenState::WaitingForAcks(KeyGenWaitingForAcks {
                                    certified_keygen,
                                    device_to_share_index: state.device_to_share_index,
                                    acks: Default::default(),
                                    pending_key_name: state.pending_key_name,
                                    purpose: state.purpose,
                                })
                            );
                        } else {
                            entry.insert(KeyGenState::WaitingForCertificates(state));
                        }
                        Ok(outgoing)
                    }
                    _ => Err(Error::coordinator_invalid_message(
                        message_kind,
                        "received VRF proof for keygen but this keygen wasn't in WaitingForCertificates state",
                    )),
                }
            }
            DeviceToCoordinatorMessage::KeyGen(keygen::DeviceKeygen::Ack(ack)) => {
                let keygen_id = ack.keygen_id;
                if self.is_remote_keygen_active(keygen_id) {
                    return self.receive_device_keygen_ack(from, ack);
                }
                let ack_session_hash = ack.ack_session_hash;
                let mut outgoing = vec![];
                let (state, entry) = self.pending_keygens.take_entry(keygen_id);

                match state {
                    Some(KeyGenState::WaitingForAcks(mut state)) => {
                        let session_hash =
                            SessionHash::from_certified_keygen(&state.certified_keygen);
                        if ack_session_hash != session_hash {
                            entry.insert(KeyGenState::WaitingForAcks(state));
                            return Err(Error::coordinator_invalid_message(
                                message_kind,
                                "Device acked wrong keygen session hash",
                            ));
                        }

                        if !state.device_to_share_index.contains_key(&from) {
                            entry.insert(KeyGenState::WaitingForAcks(state));
                            return Err(Error::coordinator_invalid_message(
                                message_kind,
                                "Received ack from device not a member of keygen",
                            ));
                        }

                        if state.acks.insert(from) {
                            let all_acks_received =
                                state.acks.len() == state.device_to_share_index.len();
                            if all_acks_received {
                                // XXX: we don't keep around the certified keygen for anything,
                                // although it would make sense for settings where the secret key for
                                // the DeviceId is persisted -- this would allow them to recover their
                                // secret share from the certified keygen.
                                let root_shared_key = state
                                    .certified_keygen
                                    .agg_input()
                                    .shared_key()
                                    .non_zero()
                                    .expect("can't be zero we we contributed to it");

                                entry.insert(KeyGenState::NeedsFinalize(KeyGenNeedsFinalize {
                                    root_shared_key,
                                    device_to_share_index: state.device_to_share_index,
                                    pending_key_name: state.pending_key_name,
                                    purpose: state.purpose,
                                }));
                            } else {
                                entry.insert(KeyGenState::WaitingForAcks(state));
                            }
                            outgoing.push(CoordinatorSend::ToUser(
                                CoordinatorToUserMessage::KeyGen {
                                    inner: CoordinatorToUserKeyGenMessage::KeyGenAck {
                                        from,
                                        all_acks_received,
                                    },
                                    keygen_id,
                                },
                            ));
                        } else {
                            entry.insert(KeyGenState::WaitingForAcks(state));
                        }
                        Ok(outgoing)
                    }
                    _ => Err(Error::coordinator_invalid_message(
                        message_kind,
                        "received ACK for keygen but this keygen wasn't in WaitingForAcks state",
                    )),
                }
            }
            DeviceToCoordinatorMessage::Restoration(message) => {
                self.recv_restoration_message(from, message)
            }
        }
    }

    pub fn begin_keygen(
        &mut self,
        begin_keygen: BeginKeygen,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<SendBeginKeygen, ActionError> {
        let BeginKeygen {
            device_to_share_index,
            threshold,
            key_name,
            purpose,
            keygen_id,
            devices_in_order,
        } = begin_keygen;

        if self.pending_keygens.contains_key(&keygen_id) {
            return Err(ActionError::StateInconsistent(
                "keygen with that id already in progress".into(),
            ));
        }

        // Generate coordinator keypair internally
        let coordinator_secret = Scalar::random(rng);
        let coordinator_keypair = KeyPair::new(coordinator_secret);

        let n_devices = device_to_share_index.len();

        if n_devices < threshold as usize {
            panic!(
                "caller needs to ensure that threshold < devices.len(). Tried {threshold}-of-{n_devices}",
            );
        }
        let share_receivers_enckeys = device_to_share_index
            .iter()
            .map(|(device, share_index)| (ShareIndex::from(*share_index), device.pubkey()))
            .collect::<BTreeMap<_, _>>();
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();

        let coordinator_public_keys = vec![coordinator_keypair.public_key()];

        let mut input_aggregator = certpedpop::Coordinator::new(
            threshold.into(),
            1 + n_devices as u32,
            &share_receivers_enckeys,
        );
        let (contributer, input) = certpedpop::Contributor::gen_keygen_input(
            &schnorr,
            threshold.into(),
            &share_receivers_enckeys,
            0,
            rng,
        );
        input_aggregator
            .add_input(&schnorr, 0, input)
            .expect("we just generated the input");

        self.pending_keygens.insert(
            keygen_id,
            KeyGenState::WaitingForResponses(KeyGenWaitingForResponses {
                keygen_id,
                input_aggregator,
                device_to_share_index: device_to_share_index.clone(),
                pending_key_name: key_name.clone(),
                purpose,
                contributer: Box::new(contributer),
                my_keypair: coordinator_keypair,
                coordinator_public_keys: coordinator_public_keys.clone(),
            }),
        );

        let begin_message = keygen::Begin {
            devices: devices_in_order,
            threshold,
            key_name,
            purpose,
            keygen_id,
            coordinator_public_keys,
        };

        Ok(SendBeginKeygen(begin_message))
    }

    pub fn finalize_keygen(
        &mut self,
        keygen_id: KeygenId,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<SendFinalizeKeygen, ActionError> {
        // Check local keygen first
        if let Some(KeyGenState::NeedsFinalize(finalize)) = self.pending_keygens.remove(&keygen_id)
        {
            return self.finalize_keygen_inner(finalize, keygen_id, encryption_key, rng);
        }

        if self.is_remote_keygen_active(keygen_id) {
            return self.finalize_remote_keygen(keygen_id, encryption_key, rng);
        }

        Err(ActionError::StateInconsistent("no such keygen".into()))
    }

    fn finalize_keygen_inner(
        &mut self,
        finalize: KeyGenNeedsFinalize,
        keygen_id: KeygenId,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<SendFinalizeKeygen, ActionError> {
        let device_to_share_index_converted = finalize
            .device_to_share_index
            .iter()
            .map(|(device, share_index)| (*device, ShareIndex::from(*share_index)))
            .collect();
        let access_structure_ref = self.mutate_new_key(
            finalize.pending_key_name,
            finalize.root_shared_key,
            device_to_share_index_converted,
            encryption_key,
            finalize.purpose,
            rng,
        );
        Ok(SendFinalizeKeygen {
            devices: finalize.device_to_share_index.into_keys().collect(),
            access_structure_ref,
            keygen_id,
        })
    }

    pub fn maybe_request_nonce_replenishment(
        &self,
        devices: &BTreeSet<DeviceId>,
        desired_nonce_streams: usize,
        rng: &mut impl rand_core::RngCore,
    ) -> NonceReplenishRequest {
        let replenish_requests = devices
            .iter()
            .map(|device_id| {
                (
                    *device_id,
                    self.signing
                        .nonce_cache
                        .generate_nonce_stream_opening_requests(
                            *device_id,
                            desired_nonce_streams,
                            rng,
                        )
                        .into_iter()
                        .collect(),
                )
            })
            .collect();

        NonceReplenishRequest { replenish_requests }
    }

    pub fn verify_address(
        &self,
        key_id: KeyId,
        derivation_index: u32,
    ) -> Result<VerifyAddress, ActionError> {
        let frost_key = self
            .get_frost_key(key_id)
            .ok_or(ActionError::StateInconsistent("no such frost key".into()))?;

        let master_appkey = frost_key.complete_key.master_appkey;

        // verify on any device that knows about this key
        let target_devices: BTreeSet<_> = frost_key
            .access_structures()
            .flat_map(|accss| {
                accss
                    .device_to_share_index
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();

        Ok(VerifyAddress {
            master_appkey,
            derivation_index,
            target_devices,
        })
    }

    pub fn nonces_available(&self, device_id: DeviceId) -> BTreeMap<NonceStreamId, u32> {
        self.signing
            .nonce_cache
            .nonces_available(device_id, &self.signing.all_used_nonce_streams())
    }

    pub fn get_access_structure(
        &self,
        access_structure_ref: AccessStructureRef,
    ) -> Option<CoordAccessStructure> {
        let key = self.keys.get(&access_structure_ref.key_id)?;
        let access_structure =
            key.get_access_structure(access_structure_ref.access_structure_id)?;
        Some(access_structure)
    }

    fn mutate_new_key(
        &mut self,
        name: String,
        root_shared_key: SharedKey,
        device_to_share_index: BTreeMap<DeviceId, ShareIndex>,
        encryption_key: SymmetricKey,
        purpose: KeyPurpose,
        rng: &mut impl rand_core::RngCore,
    ) -> AccessStructureRef {
        let rootkey = root_shared_key.public_key();
        let root_shared_key = Xpub::from_rootkey(root_shared_key);
        let app_shared_key = root_shared_key.rootkey_to_master_appkey();
        let access_structure_id =
            AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());
        let encrypted_rootkey = Ciphertext::encrypt(encryption_key, &rootkey, rng);
        let master_appkey = MasterAppkey::from_xpub_unchecked(&app_shared_key);
        let key_id = master_appkey.key_id();
        let access_structure_ref = AccessStructureRef {
            key_id,
            access_structure_id,
        };

        if self.get_frost_key(key_id).is_none() {
            self.mutate(Mutation::Keygen(keys::KeyMutation::NewKey {
                key_name: name,
                purpose,
                complete_key: CompleteKey {
                    master_appkey,
                    encrypted_rootkey,
                    access_structures: Default::default(),
                },
            }));
        }

        self.mutate(Mutation::Keygen(keys::KeyMutation::NewAccessStructure {
            shared_key: app_shared_key,
            kind: AccessStructureKind::Master,
        }));

        for (device_id, share_index) in device_to_share_index {
            self.mutate(Mutation::Keygen(keys::KeyMutation::NewShare {
                access_structure_ref,
                device_id,
                share_index,
            }));
        }

        access_structure_ref
    }

    pub fn add_key_and_access_structure(
        &mut self,
        key_name: String,
        root_shared_key: SharedKey,
        purpose: KeyPurpose,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> AccessStructureRef {
        self.mutate_new_key(
            key_name,
            root_shared_key,
            BTreeMap::new(),
            encryption_key,
            purpose,
            rng,
        )
    }

    pub fn delete_key(&mut self, key_id: KeyId) {
        if self.keys.contains_key(&key_id) {
            self.mutate(Mutation::Keygen(keys::KeyMutation::DeleteKey(key_id)));
        }
    }

    pub fn delete_share(
        &mut self,
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
    ) -> Result<(), DeleteShareError> {
        for session in self.signing.active_signing_sessions.values() {
            if session.access_structure_ref() == access_structure_ref {
                let is_involved = session.sent_req_to_device.contains(&device_id)
                    || session.has_received_from(device_id);
                if is_involved {
                    return Err(DeleteShareError::DeviceInActiveSignSession);
                }
            }
        }

        if let Some(key) = self.keys.get(&access_structure_ref.key_id) {
            if let Some(access_structure) = key
                .complete_key
                .access_structures
                .get(&access_structure_ref.access_structure_id)
            {
                if access_structure
                    .device_to_share_index
                    .contains_key(&device_id)
                {
                    self.mutate(Mutation::Keygen(keys::KeyMutation::DeleteShare {
                        access_structure_ref,
                        device_id,
                    }));
                }
            }
        }
        Ok(())
    }

    pub fn cancel_keygen(&mut self, keygen_id: KeygenId) {
        let _ = self.pending_keygens.remove(&keygen_id);
    }

    pub fn clear_tmp_data(&mut self) {
        self.pending_keygens.clear();
        self.restoration.clear_tmp_data();
        self.remote_signing.clear_tmp_data();
        self.remote_keygen.clear_tmp_data();
    }

    pub fn knows_about_share(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        index: ShareIndex,
    ) -> bool {
        let already_got_under_key = self
            .keys
            .get(&access_structure_ref.key_id)
            .and_then(|coord_key| {
                let access_structure_id = access_structure_ref.access_structure_id;
                let existing_index = coord_key
                    .get_access_structure(access_structure_id)?
                    .device_to_share_index
                    .get(&device_id)
                    .copied();

                Some(existing_index == Some(index))
            })
            .unwrap_or(false);

        already_got_under_key
    }

    pub fn root_shared_key(
        &self,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
    ) -> Option<SharedKey> {
        let frost_key = self.get_frost_key(access_structure_ref.key_id)?;

        let root_shared_key = frost_key
            .complete_key
            .root_shared_key(access_structure_ref.access_structure_id, encryption_key)?;
        Some(root_shared_key)
    }

    pub fn expected_share_image(
        &self,
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        encryption_key: SymmetricKey,
    ) -> Option<ShareImage> {
        let root_shared_key = self.root_shared_key(access_structure_ref, encryption_key)?;
        Some(root_shared_key.share_image(share_index))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenWaitingForResponses {
    pub keygen_id: KeygenId,
    pub input_aggregator: certpedpop::Coordinator,
    pub device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
    pub pending_key_name: String,
    pub purpose: KeyPurpose,
    pub contributer: Box<certpedpop::Contributor>,
    pub my_keypair: KeyPair,
    pub coordinator_public_keys: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenWaitingForCertificates {
    pub keygen_id: KeygenId,
    pub device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
    pub pending_key_name: String,
    pub purpose: KeyPurpose,
    pub certifier: certpedpop::Certifier<certpedpop::vrf_cert::VrfCertScheme<Sha256>>,
    pub coordinator_keypair: KeyPair,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenWaitingForAcks {
    pub certified_keygen: certpedpop::CertifiedKeygen<certpedpop::vrf_cert::CertVrfProof>,
    pub device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
    pub acks: BTreeSet<DeviceId>,
    pub pending_key_name: String,
    pub purpose: KeyPurpose,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenNeedsFinalize {
    pub root_shared_key: SharedKey,
    pub device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
    pub pending_key_name: String,
    pub purpose: KeyPurpose,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyGenState {
    WaitingForResponses(KeyGenWaitingForResponses),
    WaitingForCertificates(KeyGenWaitingForCertificates),
    WaitingForAcks(KeyGenWaitingForAcks),
    NeedsFinalize(KeyGenNeedsFinalize),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CoordAccessStructure {
    app_shared_key: Xpub<SharedKey>,
    device_to_share_index: BTreeMap<DeviceId, ShareIndex>,
    kind: crate::AccessStructureKind,
}

impl CoordAccessStructure {
    pub fn new(
        app_shared_key: Xpub<SharedKey>,
        device_to_share_index: BTreeMap<DeviceId, ShareIndex>,
        kind: crate::AccessStructureKind,
    ) -> Self {
        Self {
            app_shared_key,
            device_to_share_index,
            kind,
        }
    }

    pub fn threshold(&self) -> u16 {
        self.app_shared_key
            .key
            .threshold()
            .try_into()
            .expect("threshold too large")
    }

    pub fn access_structure_ref(&self) -> AccessStructureRef {
        AccessStructureRef {
            key_id: self.master_appkey().key_id(),
            access_structure_id: self.access_structure_id(),
        }
    }

    pub fn app_shared_key(&self) -> Xpub<SharedKey> {
        self.app_shared_key.clone()
    }

    pub fn master_appkey(&self) -> MasterAppkey {
        MasterAppkey::from_xpub_unchecked(&self.app_shared_key)
    }

    pub fn devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_to_share_index.keys().cloned()
    }

    pub fn contains_device(&self, id: DeviceId) -> bool {
        self.device_to_share_index.contains_key(&id)
    }

    pub fn access_structure_id(&self) -> AccessStructureId {
        AccessStructureId::from_app_poly(self.app_shared_key.key.point_polynomial())
    }

    pub fn kind(&self) -> AccessStructureKind {
        self.kind
    }

    pub fn device_to_share_indicies(&self) -> BTreeMap<DeviceId, ShareIndex> {
        self.device_to_share_index.clone()
    }

    pub fn share_index_to_devices(&self) -> BTreeMap<ShareIndex, Vec<DeviceId>> {
        let mut map = BTreeMap::new();
        for (&device_id, &share_index) in &self.device_to_share_index {
            map.entry(share_index)
                .or_insert_with(Vec::new)
                .push(device_id);
        }
        map
    }

    pub fn devices_by_share_index(&self) -> Vec<DeviceId> {
        self.share_index_to_devices()
            .into_values()
            .flatten()
            .collect()
    }

    pub fn iter_shares(&self) -> impl Iterator<Item = (DeviceId, ShareIndex)> + '_ {
        self.device_to_share_index
            .iter()
            .map(|(&device_id, &share_index)| (device_id, share_index))
    }
}

#[derive(Debug, Clone)]
pub enum DeleteShareError {
    DeviceInActiveSignSession,
}

impl fmt::Display for DeleteShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeleteShareError::DeviceInActiveSignSession => {
                write!(
                    f,
                    "Cannot delete device while it is involved in an active signing session"
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DeleteShareError {}

/// Mutations to the coordinator state
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, Kind)]
pub enum Mutation {
    #[delegate_kind]
    Keygen(keys::KeyMutation),
    #[delegate_kind]
    Signing(signing::SigningMutation),
    #[delegate_kind]
    Restoration(restoration::RestorationMutation),
    #[delegate_kind]
    RemoteSigning(remote_signing::RemoteSigningMutation),
}

impl Mutation {
    pub fn tied_to_key(&self, coord: &FrostCoordinator) -> Option<KeyId> {
        Some(match self {
            Mutation::Keygen(keys::KeyMutation::NewKey { complete_key, .. }) => {
                complete_key.master_appkey.key_id()
            }
            Mutation::Keygen(keys::KeyMutation::NewAccessStructure { shared_key, .. }) => {
                MasterAppkey::from_xpub_unchecked(shared_key).key_id()
            }
            Mutation::Keygen(keys::KeyMutation::NewShare {
                access_structure_ref,
                ..
            }) => access_structure_ref.key_id,
            Mutation::Keygen(keys::KeyMutation::DeleteKey(key_id)) => *key_id,
            Mutation::Keygen(keys::KeyMutation::DeleteShare {
                access_structure_ref,
                ..
            }) => access_structure_ref.key_id,
            Mutation::Signing(inner) => inner.tied_to_key(coord)?,
            Mutation::RemoteSigning(_) => return None,
            Mutation::Restoration(inner) => inner.tied_to_key()?,
        })
    }

    pub fn tied_to_restoration(&self) -> Option<RestorationId> {
        match self {
            Mutation::Restoration(mutation) => mutation.tied_to_restoration(),
            _ => None,
        }
    }
}

impl Gist for Mutation {
    fn gist(&self) -> String {
        crate::Kind::kind(self).into()
    }
}

#[derive(Clone, Debug)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum CoordinatorSend {
    ToDevice {
        message: CoordinatorToDeviceMessage,
        destinations: BTreeSet<DeviceId>,
    },
    ToUser(CoordinatorToUserMessage),
    Broadcast {
        /// Identifies which in-flight session this broadcast belongs to.
        /// Today the only broadcast payload is a remote keygen, so this
        /// is always a `KeygenId`. If we ever get a second broadcast
        /// session type (e.g. a group signing protocol), this field
        /// should be generalized to a `ChannelId` newtype that both
        /// session kinds can be converted into.
        channel: KeygenId,
        from: DeviceId,
        payload: BroadcastPayload,
    },
}

#[derive(Clone, Debug)]
pub enum BroadcastPayload {
    RemoteKeygen(remote_keygen::RemoteKeygenPayload),
}

#[derive(Debug, Clone)]
pub struct VerifyAddress {
    pub master_appkey: MasterAppkey,
    pub derivation_index: u32,
    pub target_devices: BTreeSet<DeviceId>,
}

impl IntoIterator for VerifyAddress {
    type Item = CoordinatorSend;
    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::ScreenVerify(
                crate::message::screen_verify::ScreenVerify::VerifyAddress {
                    master_appkey: self.master_appkey,
                    derivation_index: self.derivation_index,
                },
            ),
            destinations: self.target_devices,
        })
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SendBeginKeygen(pub keygen::Begin);

impl IntoIterator for SendBeginKeygen {
    type Item = CoordinatorSend;
    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(CoordinatorSend::ToDevice {
            destinations: self.0.devices.iter().cloned().collect(),
            message: self.0.into(),
        })
    }
}

pub struct SendFinalizeKeygen {
    pub devices: Vec<DeviceId>,
    pub access_structure_ref: AccessStructureRef,
    pub keygen_id: KeygenId,
}

impl IntoIterator for SendFinalizeKeygen {
    type Item = CoordinatorSend;
    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(CoordinatorSend::ToDevice {
            message: keygen::Keygen::Finalize {
                keygen_id: self.keygen_id,
            }
            .into(),
            destinations: self.devices.into_iter().collect(),
        })
    }
}
