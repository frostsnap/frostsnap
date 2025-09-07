use crate::{device::KeyPurpose, AccessStructureKind, AccessStructureRef, KeyId, Kind};
use alloc::{boxed::Box, string::String};
use frostsnap_macros::Kind as KindDerive;

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, KindDerive)]
pub enum KeyMutation {
    NewKey {
        key_id: KeyId,
        key_name: String,
        purpose: KeyPurpose,
    },
    NewAccessStructure {
        access_structure_ref: AccessStructureRef,
        threshold: u16,
        kind: AccessStructureKind,
    },
    SaveShare(Box<super::SaveShareMutation>),
}
