use crate::{
    device::KeyPurpose, AccessStructureRef, AccessStructureKind, KeyId, Kind,
};
use alloc::{string::String, boxed::Box};
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