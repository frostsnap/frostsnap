use crate::coordinator::CompleteKey;
use crate::tweak::Xpub;
use crate::{
    device::KeyPurpose, AccessStructureKind, AccessStructureRef, DeviceId, KeyId, KeygenId, Kind,
};
use alloc::{collections::BTreeMap, collections::BTreeSet, string::String, vec::Vec};
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::frost::{ShareIndex, SharedKey};

/// API input for beginning a key generation (without coordinator_public_key)
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
        rng: &mut impl rand_core::RngCore, // for the keygen id
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
