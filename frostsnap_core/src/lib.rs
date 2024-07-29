#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;
pub mod encrypted_share;
mod key_id;
mod macros;
pub mod message;
pub mod nostr;
pub mod tweak;

use coordinator::CoordinatorState;
pub use sha2;
mod sign_task;
pub use sign_task::*;

pub use bincode;
pub use key_id::*;
pub use serde;
pub mod coordinator;
mod device;
pub use device::*;
pub use schnorr_fun;
pub mod bitcoin_transaction;

#[cfg(feature = "rusqlite")]
mod sqlite;

#[macro_use]
extern crate alloc;

use crate::message::*;
use alloc::{string::String, string::ToString, vec::Vec};
use schnorr_fun::fun::{Point, Tag};
// rexport hex module so serialization impl macros work outside this crate
pub use schnorr_fun::fun::hex;
use sha2::digest::Digest;
use sha2::Sha256;

const NONCE_BATCH_SIZE: u64 = 10;

pub type SessionHash = [u8; 32];

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

    pub fn pubkey(&self) -> Option<Point> {
        Point::from_bytes(self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 33] {
        &self.0
    }
}

pub fn gen_pop_message(device_ids: impl IntoIterator<Item = DeviceId>) -> [u8; 32] {
    let mut hasher = Sha256::default().tag(b"frostsnap/pop");
    for id in device_ids {
        hasher.update(id.as_bytes());
    }
    hasher.finalize().into()
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
