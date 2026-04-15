use super::*;
use crate::{
    fail,
    symmetric_encryption::{Ciphertext, SymmetricKey},
    tweak::Xpub,
    AccessStructureId, AccessStructureKind, AccessStructureRef, ActionError, DeviceId, KeyId,
    KeygenId, Kind, MasterAppkey, SessionHash,
};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::{
    frost::{chilldkg::certpedpop, ShareIndex, SharedKey},
    fun::KeyPair,
};
use sha2::Sha256;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    keys: BTreeMap<KeyId, CoordFrostKey>,
    key_order: Vec<KeyId>,
    pending_keygens: HashMap<KeygenId, KeyGenState>,
}

impl State {
    pub fn apply_mutation_keygen(&mut self, mutation: KeyMutation) -> Option<KeyMutation> {
        match mutation {
            KeyMutation::NewKey {
                ref complete_key,
                ref key_name,
                purpose,
            } => {
                let key_id = complete_key.master_appkey.key_id();
                let exists = self.keys.contains_key(&key_id);
                self.keys.entry(key_id).or_insert_with(|| CoordFrostKey {
                    key_id,
                    complete_key: complete_key.clone(),
                    key_name: key_name.to_owned(),
                    purpose,
                });
                if !exists {
                    self.key_order.push(key_id);
                }
            }
            KeyMutation::NewAccessStructure {
                ref shared_key,
                kind,
            } => {
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
            KeyMutation::NewShare {
                access_structure_ref,
                device_id,
                share_index,
            } => match self.keys.get_mut(&access_structure_ref.key_id) {
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
            KeyMutation::DeleteShare {
                access_structure_ref,
                device_id,
            } => match self.keys.get_mut(&access_structure_ref.key_id) {
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
            KeyMutation::DeleteKey(key_id) => {
                self.keys.remove(&key_id)?;
                self.key_order.retain(|&entry| entry != key_id);
            }
        }

        Some(mutation)
    }

    pub fn get_key(&self, key_id: KeyId) -> Option<&CoordFrostKey> {
        self.keys.get(&key_id)
    }

    pub fn iter_keys(&self) -> impl Iterator<Item = &CoordFrostKey> + '_ {
        self.key_order
            .iter()
            .map(|key_id| self.keys.get(key_id).expect("invariant"))
    }

    pub fn iter_access_structures(&self) -> impl Iterator<Item = CoordAccessStructure> + '_ {
        self.keys
            .iter()
            .flat_map(|(_, key_data)| key_data.access_structures())
    }

    pub fn clear_tmp_data(&mut self) {
        self.pending_keygens.clear();
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
        restoration_id: crate::RestorationId,
    },
}

impl CompleteKey {
    pub fn coord_share_decryption_contrib(
        &self,
        access_structure_id: AccessStructureId,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> Option<(Point, crate::CoordShareDecryptionContrib)> {
        let root_shared_key = self.root_shared_key(access_structure_id, encryption_key)?;
        let share_index = *self
            .access_structures
            .get(&access_structure_id)?
            .device_to_share_index
            .get(&device_id)?;
        Some((
            root_shared_key.public_key(),
            crate::CoordShareDecryptionContrib::for_master_share(
                device_id,
                share_index,
                &root_shared_key,
            ),
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

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct CoordAccessStructure {
    app_shared_key: Xpub<SharedKey>,
    pub(super) device_to_share_index: BTreeMap<DeviceId, ShareIndex>,
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

#[derive(Clone, Debug)]
pub struct BeginKeygen {
    pub keygen_id: KeygenId,
    pub devices_in_order: Vec<DeviceId>,
    pub device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
    pub threshold: u16,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

impl BeginKeygen {
    pub fn new_with_id(
        devices: Vec<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        keygen_id: KeygenId,
    ) -> Self {
        let device_to_share_index: BTreeMap<_, _> = devices
            .iter()
            .enumerate()
            .map(|(index, device_id)| {
                (
                    *device_id,
                    core::num::NonZeroU32::new((index + 1) as u32).expect("we added one"),
                )
            })
            .collect();

        Self {
            devices_in_order: devices,
            device_to_share_index,
            threshold,
            key_name,
            purpose,
            keygen_id,
        }
    }

    pub fn new(
        devices: Vec<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        rng: &mut impl rand_core::RngCore,
    ) -> Self {
        let mut id = [0u8; 16];
        rng.fill_bytes(&mut id[..]);

        Self::new_with_id(
            devices,
            threshold,
            key_name,
            purpose,
            KeygenId::from_bytes(id),
        )
    }

    pub fn devices(&self) -> BTreeSet<DeviceId> {
        self.device_to_share_index.keys().cloned().collect()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, KindDerive)]
pub enum KeyMutation {
    NewKey {
        key_name: String,
        purpose: KeyPurpose,
        complete_key: CompleteKey,
    },
    NewAccessStructure {
        shared_key: Xpub<SharedKey>,
        kind: AccessStructureKind,
    },
    NewShare {
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
        share_index: ShareIndex,
    },
    DeleteKey(KeyId),
    DeleteShare {
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
    },
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

impl FrostCoordinator {
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

        if self.keys.pending_keygens.contains_key(&keygen_id) {
            return Err(ActionError::StateInconsistent(
                "keygen with that id already in progress".into(),
            ));
        }

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
        let mut input_aggregator = certpedpop::Coordinator::new(
            threshold.into(),
            (n_devices + 1) as u32,
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

        self.keys.pending_keygens.insert(
            keygen_id,
            KeyGenState::WaitingForResponses(KeyGenWaitingForResponses {
                keygen_id,
                input_aggregator,
                device_to_share_index: device_to_share_index.clone(),
                pending_key_name: key_name.clone(),
                purpose,
                contributer: Box::new(contributer),
                my_keypair: coordinator_keypair,
            }),
        );

        let begin_message = keygen::Begin {
            devices: devices_in_order,
            threshold,
            key_name,
            purpose,
            keygen_id,
            coordinator_public_key: coordinator_keypair.public_key(),
        };

        Ok(SendBeginKeygen(begin_message))
    }

    pub fn finalize_keygen(
        &mut self,
        keygen_id: KeygenId,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<SendFinalizeKeygen, ActionError> {
        match self.keys.pending_keygens.remove(&keygen_id) {
            Some(KeyGenState::NeedsFinalize(finalize)) => {
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
            _ => Err(ActionError::StateInconsistent("no such keygen".into())),
        }
    }

    pub fn cancel_keygen(&mut self, keygen_id: KeygenId) {
        let _ = self.keys.pending_keygens.remove(&keygen_id);
    }

    pub(super) fn mutate_new_key(
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
            self.mutate(Mutation::Keygen(KeyMutation::NewKey {
                key_name: name,
                purpose,
                complete_key: CompleteKey {
                    master_appkey,
                    encrypted_rootkey,
                    access_structures: Default::default(),
                },
            }));
        }

        self.mutate(Mutation::Keygen(KeyMutation::NewAccessStructure {
            shared_key: app_shared_key,
            kind: AccessStructureKind::Master,
        }));

        for (device_id, share_index) in device_to_share_index {
            self.mutate(Mutation::Keygen(KeyMutation::NewShare {
                access_structure_ref,
                device_id,
                share_index,
            }));
        }

        access_structure_ref
    }

    pub fn delete_key(&mut self, key_id: KeyId) {
        if self.get_frost_key(key_id).is_some() {
            self.mutate(Mutation::Keygen(KeyMutation::DeleteKey(key_id)));
        }
    }

    pub fn recv_keygen_message(
        &mut self,
        from: DeviceId,
        message: keygen::DeviceKeygen,
        message_kind: &'static str,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        match message {
            keygen::DeviceKeygen::Response(response) => {
                let keygen_id = response.keygen_id;
                let (state, entry) = self.keys.pending_keygens.take_entry(keygen_id);

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

                        // we use the share index as the input generator index. The input
                        // generator at index 0 is the coordinator itself.
                        state
                            .input_aggregator
                            .add_input(
                                &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                                (*share_index).into(),
                                *response.input,
                            )
                            .map_err(|e| Error::coordinator_invalid_message(message_kind, e))?;

                        let mut outgoing =
                            vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                                keygen_id,
                                inner: CoordinatorToUserKeyGenMessage::ReceivedShares { from },
                            })];

                        if state.input_aggregator.is_finished() {
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
                                &[state.my_keypair.public_key()],
                            );

                            certifier
                                .receive_certificate(state.my_keypair.public_key(), sig)
                                .expect("will be able to verify our own certificate");

                            outgoing.push(CoordinatorSend::ToDevice {
                                destinations: state.device_to_share_index.keys().cloned().collect(),
                                message: keygen::Keygen::CertifyPlease {
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
            keygen::DeviceKeygen::Certify {
                keygen_id,
                vrf_cert,
            } => {
                let mut outgoing = vec![];
                let (state, entry) = self.keys.pending_keygens.take_entry(keygen_id);

                match state {
                    Some(KeyGenState::WaitingForCertificates(mut state)) => {
                        state
                            .certifier
                            .receive_certificate(from.pubkey(), vrf_cert)
                            .map_err(|_| {
                                Error::coordinator_invalid_message(
                                    message_kind,
                                    "Invalid VRF proof received",
                                )
                            })?;

                        // contributers are the devices plus one coordinator
                        if state.certifier.is_finished() {
                            let certified_keygen =
                                state.certifier.finish().expect("just checked is_finished");

                            let session_hash =
                                SessionHash::from_certified_keygen(&certified_keygen);

                            let certificate = certified_keygen
                                .certificate()
                                .iter()
                                .map(|(pk, cert)| (*pk, cert.clone()))
                                .collect();

                            outgoing.push(CoordinatorSend::ToDevice {
                                destinations: state.device_to_share_index.keys().cloned().collect(),
                                message: keygen::Keygen::Check {
                                    keygen_id,
                                    certificate,
                                }
                                .into(),
                            });

                            outgoing.push(CoordinatorSend::ToUser(
                                CoordinatorToUserMessage::KeyGen {
                                    keygen_id,
                                    inner: CoordinatorToUserKeyGenMessage::CheckKeyGen {
                                        session_hash,
                                    },
                                },
                            ));

                            entry.insert(KeyGenState::WaitingForAcks(KeyGenWaitingForAcks {
                                certified_keygen,
                                device_to_share_index: state.device_to_share_index,
                                acks: Default::default(),
                                pending_key_name: state.pending_key_name,
                                purpose: state.purpose,
                            }));
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
            keygen::DeviceKeygen::Ack(KeyGenAck {
                keygen_id,
                ack_session_hash,
            }) => {
                let mut outgoing = vec![];
                let (state, entry) = self.keys.pending_keygens.take_entry(keygen_id);

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
        }
    }
}
