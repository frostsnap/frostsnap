use crate::{
    device::{KeyPurpose, NONCE_BATCH_SIZE},
    map_ext::*,
    message::*,
    nonce_stream::NonceStreamId,
    symmetric_encryption::SymmetricKey,
    AccessStructureRef, ActionError, DeviceId, Error, Gist, KeyId, KeygenId, Kind, MasterAppkey,
    MessageResult, RestorationId, SessionHash, ShareImage, SignSessionId,
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
    frost::{ShareIndex, SharedKey},
    fun::prelude::*,
};
use tracing::{event, Level};

mod coordinator_to_user;
pub mod keys;
pub mod restoration;
pub mod signing;
pub use coordinator_to_user::*;
pub use keys::{
    BeginKeygen, CompleteKey, CoordAccessStructure, CoordFrostKey, DeleteShareError,
    KeyGenNeedsFinalize, KeyGenState, KeyGenWaitingForAcks, KeyGenWaitingForCertificates,
    KeyGenWaitingForResponses, KeyLocationState, KeyMutation, SendBeginKeygen, SendFinalizeKeygen,
    ShareLocation,
};
pub use signing::{
    ActiveSignSession, FinishedSignSession, NonceReplenishRequest, RequestDeviceSign, SignSession,
    SignSessionProgress, SigningMutation, StartSign, StartSignError,
};

pub const MIN_NONCES_BEFORE_REQUEST: u32 = NONCE_BATCH_SIZE / 2;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrostCoordinator {
    keys: keys::State,
    mutations: VecDeque<Mutation>,
    signing: signing::State,
    restoration: restoration::State,
    pub keygen_fingerprint: schnorr_fun::frost::Fingerprint,
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
        use Mutation::*;
        match mutation {
            Keygen(keys::KeyMutation::DeleteKey(key_id)) => {
                self.keys
                    .apply_mutation_keygen(keys::KeyMutation::DeleteKey(key_id))?;
                self.restoration.clear_up_key_deletion(key_id);
                self.signing.clear_up_key_deletion(key_id);
            }
            Keygen(inner) => {
                return self.keys.apply_mutation_keygen(inner).map(Mutation::Keygen);
            }
            Signing(inner) => {
                return self
                    .signing
                    .apply_mutation_signing(inner)
                    .map(Mutation::Signing);
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
        self.keys.iter_keys()
    }

    pub fn iter_access_structures(&self) -> impl Iterator<Item = CoordAccessStructure> + '_ {
        self.keys.iter_access_structures()
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
                        .get_frost_key(access_structure_ref.key_id)
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
            .iter_restorations()
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
                .get_frost_key(access_structure_ref.key_id)
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
        for restoration in self.restoration.iter_restorations() {
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
        self.keys.get_key(key_id)
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
            DeviceToCoordinatorMessage::KeyGen(message) => {
                self.recv_keygen_message(from, message, message_kind)
            }
            DeviceToCoordinatorMessage::Restoration(message) => {
                self.recv_restoration_message(from, message)
            }
        }
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
        self.signing.nonces_available(device_id)
    }

    pub fn get_access_structure(
        &self,
        access_structure_ref: AccessStructureRef,
    ) -> Option<CoordAccessStructure> {
        let key = self.get_frost_key(access_structure_ref.key_id)?;
        let access_structure =
            key.get_access_structure(access_structure_ref.access_structure_id)?;
        Some(access_structure)
    }

    pub fn delete_share(
        &mut self,
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
    ) -> Result<(), DeleteShareError> {
        if self
            .signing
            .is_device_in_active_session(access_structure_ref, device_id)
        {
            return Err(DeleteShareError::DeviceInActiveSignSession);
        }

        if let Some(key) = self.get_frost_key(access_structure_ref.key_id) {
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

    pub fn clear_tmp_data(&mut self) {
        self.keys.clear_tmp_data();
        self.signing.clear_tmp_data();
        self.restoration.clear_tmp_data();
    }

    pub fn knows_about_share(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        index: ShareIndex,
    ) -> bool {
        let already_got_under_key = self
            .get_frost_key(access_structure_ref.key_id)
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
}

/// Mutations to the coordinator state
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, Kind)]
pub enum Mutation {
    #[delegate_kind]
    Keygen(keys::KeyMutation),
    #[delegate_kind]
    Signing(signing::SigningMutation),
    #[delegate_kind]
    Restoration(restoration::RestorationMutation),
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
            Mutation::Keygen(keys::KeyMutation::DeleteShare {
                access_structure_ref,
                ..
            }) => access_structure_ref.key_id,
            Mutation::Keygen(keys::KeyMutation::DeleteKey(key_id)) => *key_id,
            Mutation::Signing(inner) => inner.tied_to_key(coord)?,
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
pub enum CoordinatorSend {
    ToDevice {
        message: CoordinatorToDeviceMessage,
        destinations: BTreeSet<DeviceId>,
    },
    ToUser(CoordinatorToUserMessage),
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
