#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;
mod appkey;
mod macros;
pub mod message;
pub mod nostr;
pub mod tweak;

use coordinator::CoordinatorState;
use device::SignerState;
use schnorr_fun::{
    frost::{chilldkg::encpedpop, SharedKey},
    fun::{hash::HashAdd, prelude::*},
};
pub use sha2;
mod sign_task;
use sha2::{digest::FixedOutput, Digest};
pub use sign_task::*;

pub use appkey::*;
pub use bincode;
pub use serde;
pub mod coordinator;
pub mod device;
pub use schnorr_fun;
pub mod bitcoin_transaction;
mod symmetric_encryption;
pub use symmetric_encryption::*;

#[cfg(feature = "rusqlite")]
mod sqlite;

#[macro_use]
extern crate alloc;

use crate::message::*;
use alloc::{string::String, string::ToString, vec::Vec};
// rexport hex module so serialization impl macros work outside this crate
pub use schnorr_fun::fun::hex;

const NONCE_BATCH_SIZE: u64 = 10;

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
    pub fn coordinator_message_kind(state: &Option<CoordinatorState>, kind: &'static str) -> Self {
        Self::MessageKind {
            state: state.as_ref().map(|x| x.name()).unwrap_or("None"),
            kind,
        }
    }

    pub fn signer_message_kind(
        state: &Option<SignerState>,
        message: &CoordinatorToDeviceMessage,
    ) -> Self {
        Self::MessageKind {
            state: state.as_ref().map(|x| x.name()).unwrap_or("None"),
            kind: message.kind(),
        }
    }

    pub fn coordinator_invalid_message(kind: &'static str, reason: impl ToString) -> Self {
        Self::InvalidMessage {
            kind,
            reason: reason.to_string(),
        }
    }

    pub fn signer_invalid_message(
        message: &CoordinatorToDeviceMessage,
        reason: impl ToString,
    ) -> Self {
        Self::InvalidMessage {
            kind: message.kind(),
            reason: reason.to_string(),
        }
    }

    pub fn signer_message_error(message: &CoordinatorToDeviceMessage, e: impl ToString) -> Self {
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
    WrongState {
        in_state: &'static str,
        action: &'static str,
    },
    StateInconsistent(String),
}

impl core::fmt::Display for ActionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ActionError::WrongState { in_state, action } => {
                write!(f, "Can not {} while in {}", action, in_state)
            }
            ActionError::StateInconsistent(error) => {
                write!(f, "action state inconsistent: {error}")
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

/// The hash of a threshold access structure for a particualr key
#[derive(Clone, Copy, PartialEq, Ord, PartialOrd, Eq)]
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
#[derive(Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
pub struct KeyId([u8; 32]);

impl KeyId {
    pub fn from_rootkey(point: Point<Normal, Public, impl ZeroChoice>) -> Self {
        Self(prefix_hash("KEY_ID").add(point).finalize_fixed().into())
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
    hash.update((prefix.len() as u32).to_be_bytes());
    hash.update(prefix);
    hash
}

#[derive(Clone, Copy, PartialEq)]
/// This is the data provided by the coordinator that helps the device decrypt their share.
/// Devices can't decrypt their shares on their own.
pub struct CoordShareDecryptionContrib([u8; 32]);

impl CoordShareDecryptionContrib {
    pub fn from_root_shared_key(shared_key: &SharedKey<Normal, impl ZeroChoice>) -> Self {
        Self(
            prefix_hash("SHARE_DECRYPTION")
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
