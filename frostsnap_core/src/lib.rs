#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;
#[cfg(feature = "coordinator")]
pub mod coord_nonces;
pub mod device_nonces;
mod macros;
mod master_appkey;
pub mod message;
pub mod nonce_stream;
pub mod nostr;
pub mod tweak;

use core::ops::RangeBounds;

use schnorr_fun::{
    frost::{self, chilldkg::encpedpop, PartyIndex, SharedKey},
    fun::{hash::HashAdd, prelude::*},
};
pub use sha2;
mod sign_task;
use sha2::{digest::FixedOutput, Digest};
pub use sign_task::*;

pub use bincode;
pub use master_appkey::*;
pub use serde;
#[cfg(feature = "coordinator")]
pub mod coordinator;
pub mod device;
pub use schnorr_fun;
pub mod bitcoin_transaction;
mod symmetric_encryption;
pub use symmetric_encryption::*;
use tweak::Xpub;

#[cfg(feature = "rusqlite")]
mod sqlite;

#[macro_use]
extern crate alloc;

use alloc::{string::String, string::ToString, vec::Vec};
// rexport hex module so serialization impl macros work outside this crate
pub use schnorr_fun::fun::hex;

const NONCE_BATCH_SIZE: u32 = 10;

#[derive(Clone, Copy, PartialEq, Hash, Eq, Ord, PartialOrd)]
pub struct DeviceId(pub [u8; 33]);

impl Default for DeviceId {
    fn default() -> Self {
        Self([0u8; 33])
    }
}

impl_display_debug_serialize! {
    fn to_bytes(device_id: &DeviceId) -> [u8;33] {
        device_id.0
    }
}

impl_fromstr_deserialize! {

    name => "device id",
    fn from_bytes(bytes: [u8;33]) -> DeviceId {
        DeviceId(bytes)
    }
}

impl DeviceId {
    pub fn new(point: Point) -> Self {
        Self(point.to_bytes())
    }

    pub fn pubkey(&self) -> Point {
        // ⚠ if the device id is invalid we give it nullish public key.
        // Honest device ids will never suffer this problem
        let point = Point::from_bytes(self.0);
        debug_assert!(point.is_some());
        point.unwrap_or(schnorr_fun::fun::G.normalize())
    }

    pub fn as_bytes(&self) -> &[u8; 33] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The device was not in a state where it could receive a message of that kind
    MessageKind {
        state: &'static str,
        kind: &'static str,
    },
    /// The content of the message was invalid with respect to the state.
    InvalidMessage { kind: &'static str, reason: String },
}

impl Error {
    #[cfg(feature = "coordinator")]
    pub fn coordinator_invalid_message(kind: &'static str, reason: impl ToString) -> Self {
        Self::InvalidMessage {
            kind,
            reason: reason.to_string(),
        }
    }

    pub fn signer_invalid_message(message: &impl Kind, reason: impl ToString) -> Self {
        Self::InvalidMessage {
            kind: message.kind(),
            reason: reason.to_string(),
        }
    }

    pub fn signer_message_error(message: &impl Kind, e: impl ToString) -> Self {
        Self::InvalidMessage {
            kind: message.kind(),
            reason: e.to_string(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::MessageKind { state, kind } => write!(
                f,
                "Unexpected message of kind {} for this state {}",
                kind, state
            ),
            Error::InvalidMessage { kind, reason } => {
                write!(f, "Invalid message of kind {}: {}", kind, reason)
            }
        }
    }
}

impl Error {
    pub fn gist(&self) -> String {
        match self {
            Error::MessageKind { state, kind } => format!("mk!{} {}", kind, state),
            Error::InvalidMessage { kind, reason } => format!("im!{}: {}", kind, reason),
        }
    }
}

pub type MessageResult<T> = Result<T, Error>;

#[derive(Debug, Clone)]
pub enum DoKeyGenError {
    WrongState,
}

#[derive(Debug, Clone)]
pub enum ActionError {
    StateInconsistent(String),
}

impl core::fmt::Display for ActionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ActionError::StateInconsistent(error) => {
                write!(f, "state inconsistent: {error}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ActionError {}

/// Output very basic debug info about a type
pub trait Gist {
    fn gist(&self) -> String;
}

pub trait Kind {
    fn kind(&self) -> &'static str;
}

/// The hash of a threshold access structure for a particualr key
#[derive(Clone, Copy, PartialEq, Ord, PartialOrd, Eq, Hash)]
pub struct AccessStructureId(pub [u8; 32]);

impl AccessStructureId {
    pub fn from_app_poly(app_poly: &[Point<Normal, Public, Zero>]) -> Self {
        Self(
            prefix_hash("ACCESS_STRUCTURE_ID")
                .add(app_poly)
                .finalize_fixed()
                .into(),
        )
    }
}

impl_display_debug_serialize! {
    fn to_bytes(as_id: &AccessStructureId) -> [u8;32] {
        as_id.0
    }
}

impl_fromstr_deserialize! {
    name => "frostsnap access structure id",
    fn from_bytes(bytes: [u8;32]) -> AccessStructureId {
        AccessStructureId(bytes)
    }
}

/// The hash of a root key
#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct KeyId(pub [u8; 32]);

impl KeyId {
    pub fn from_rootkey(rootkey: Point) -> Self {
        Self::from_master_appkey(MasterAppkey::derive_from_rootkey(rootkey))
    }

    pub fn from_master_appkey(master_appkey: MasterAppkey) -> Self {
        Self(
            prefix_hash("KEY_ID")
                .add(master_appkey.0)
                .finalize_fixed()
                .into(),
        )
    }
}

impl_display_debug_serialize! {
    fn to_bytes(value: &KeyId) -> [u8;32] {
        value.0
    }
}

impl_fromstr_deserialize! {
    name => "frostsnap key id",
    fn from_bytes(bytes: [u8;32]) -> KeyId {
        KeyId(bytes)
    }
}

fn prefix_hash(prefix: &'static str) -> sha2::Sha256 {
    let mut hash = sha2::Sha256::default();
    hash.update((prefix.len() as u8).to_be_bytes());
    hash.update(prefix);
    hash
}

#[derive(Clone, Copy, PartialEq)]
/// This is the data provided by the coordinator that helps the device decrypt their share.
/// Devices can't decrypt their shares on their own.
pub struct CoordShareDecryptionContrib([u8; 32]);

impl CoordShareDecryptionContrib {
    /// Master shares are not protected by much. Devices holding master shares are designed to have
    /// their backups stored right next them anyway. We nevertheless make the coordinator provide a
    /// hash of the root polynomial. The main benefit is to force the device to be talking to a
    /// coordinator that knows about the entire access structure (knows the polynomial). This
    /// prevents us from inadvertently introducing "features" that can be engaged without actual
    /// knowledge of the main polynomial.
    pub fn for_master_share(
        device_id: DeviceId,
        share_index: PartyIndex,
        shared_key: &SharedKey<Normal, impl ZeroChoice>,
    ) -> Self {
        Self(
            prefix_hash("SHARE_DECRYPTION")
                .add(device_id.0)
                .add(share_index)
                .add(shared_key.point_polynomial())
                .finalize_fixed()
                .into(),
        )
    }
}

impl_display_debug_serialize! {
    fn to_bytes(value: &CoordShareDecryptionContrib) -> [u8;32] {
        value.0
    }
}

impl_fromstr_deserialize! {
    name => "share decryption key",
    fn from_bytes(bytes: [u8;32]) -> CoordShareDecryptionContrib {
        CoordShareDecryptionContrib(bytes)
    }
}

#[derive(Clone, Copy, PartialEq, Hash, Eq, Ord, PartialOrd)]
pub struct SessionHash(pub [u8; 32]);

impl SessionHash {
    pub fn from_agg_input(agg_input: &encpedpop::AggKeygenInput) -> Self {
        Self(
            sha2::Sha256::default()
                .chain_update(agg_input.cert_bytes())
                .finalize_fixed()
                .into(),
        )
    }
}

impl_display_debug_serialize! {
    fn to_bytes(value: &SessionHash) -> [u8;32] {
        value.0
    }
}

impl_fromstr_deserialize! {
    name => "session hash",
    fn from_bytes(bytes: [u8;32]) -> SessionHash {
        SessionHash(bytes)
    }
}

#[derive(Clone, Copy, Debug, bincode::Encode, bincode::Decode, Ord, PartialOrd, PartialEq, Eq)]
pub struct ShareImage {
    pub share_index: PartyIndex,
    pub point: Point<Normal, Public, Zero>,
}

impl ShareImage {
    pub fn from_secret(secret_share: frost::SecretShare) -> Self {
        Self {
            share_index: secret_share.index,
            point: g!(secret_share.share * G).normalize(),
        }
    }

    pub fn share_index_u16(&self) -> u16 {
        // XXX: temporary HACK
        u16::from_str_radix(&self.share_index.to_string(), 16).expect("share index is small")
    }
}
// Uniquely identifies an access structure for a particular `key_id`.
#[derive(
    Debug, Clone, Copy, bincode::Encode, bincode::Decode, PartialEq, Eq, Hash, Ord, PartialOrd,
)]
pub struct AccessStructureRef {
    pub key_id: KeyId,
    pub access_structure_id: AccessStructureId,
}

impl AccessStructureRef {
    pub fn from_root_shared_key(root_shared_key: &SharedKey<Normal>) -> Self {
        let app_shared_key = Xpub::from_rootkey(root_shared_key.clone()).rootkey_to_master_appkey();
        let master_appkey = MasterAppkey::from_xpub_unchecked(&app_shared_key);
        let access_structure_id =
            AccessStructureId::from_app_poly(app_shared_key.into_key().point_polynomial());

        AccessStructureRef {
            key_id: master_appkey.key_id(),
            access_structure_id,
        }
    }
    pub fn range_for_key(key_id: KeyId) -> impl RangeBounds<AccessStructureRef> {
        AccessStructureRef {
            key_id,
            access_structure_id: AccessStructureId([0x00u8; 32]),
        }..=AccessStructureRef {
            key_id,
            access_structure_id: AccessStructureId([0xffu8; 32]),
        }
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct SignSessionId(pub [u8; 32]);

impl_display_debug_serialize! {
    fn to_bytes(value: &SignSessionId) -> [u8;32] {
        value.0
    }
}

impl_fromstr_deserialize! {
    name => "sign session id",
    fn from_bytes(bytes: [u8;32]) -> SignSessionId {
        SignSessionId(bytes)
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub enum Versioned<T> {
    V0(T),
}

impl<T: Clone> Versioned<&T> {
    pub fn cloned(&self) -> Versioned<T> {
        match self {
            Versioned::V0(v) => Versioned::V0((*v).clone()),
        }
    }
}

/// short randomly sampled id for a coordinator to refer to a key generation session before the key
/// generation is complete.
#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash, Default)]
pub struct KeygenId(pub [u8; 16]);

impl_display_debug_serialize! {
    fn to_bytes(keygen_id: &KeygenId) -> [u8;16] {
        keygen_id.0
    }
}

impl_fromstr_deserialize! {
    name => "key generation id",
    fn from_bytes(bytes: [u8;16]) -> KeygenId {
        KeygenId(bytes)
    }
}

/// short randomly sampled id for a coordinator to refer to a key generation session before the key
/// generation is complete.
#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash, Default)]
pub struct RestorationId(pub [u8; 16]);

impl RestorationId {
    pub fn new(rng: &mut impl rand_core::RngCore) -> Self {
        let mut bytes = [0u8; 16];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }
}

impl_display_debug_serialize! {
    fn to_bytes(val: &RestorationId) -> [u8;16] {
        val.0
    }
}

impl_fromstr_deserialize! {
    name => "restoration id",
    fn from_bytes(bytes: [u8;16]) -> RestorationId {
        RestorationId(bytes)
    }
}

/// short randomly sampled id for a coordinator to refer to a physical backup entry it asked a device to do.
#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash, Default)]
pub struct EnterPhysicalId(pub [u8; 16]);

impl EnterPhysicalId {
    pub fn new(rng: &mut impl rand_core::RngCore) -> Self {
        let mut bytes = [0u8; 16];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }
}

impl_display_debug_serialize! {
    fn to_bytes(val: &EnterPhysicalId) -> [u8;16] {
        val.0
    }
}

impl_fromstr_deserialize! {
    name => "restoration id",
    fn from_bytes(bytes: [u8;16]) -> EnterPhysicalId {
        EnterPhysicalId(bytes)
    }
}

/// In case we add access structures with more restricted properties later on
#[derive(Clone, Copy, Debug, PartialEq, bincode::Decode, bincode::Encode)]
pub enum AccessStructureKind {
    Master,
}
