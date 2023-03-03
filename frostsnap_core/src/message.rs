use crate::encrypted_share::EncryptedShare;
use crate::String;
use crate::Vec;
use crate::xpub::ExtendedPubKey;
use alloc::collections::{BTreeMap, BTreeSet};
use schnorr_fun::fun::marker::Public;
use schnorr_fun::fun::marker::Zero;
use schnorr_fun::fun::Point;
use schnorr_fun::fun::Scalar;
use schnorr_fun::musig::Nonce;
use schnorr_fun::Signature;

use crate::DeviceId;

#[derive(Clone, Debug)]
pub enum DeviceSend {
    ToUser(DeviceToUserMessage),
    ToCoordinator(DeviceToCoordindatorMessage),
}

#[derive(Clone, Debug)]
pub enum CoordinatorSend {
    ToDevice(CoordinatorToDeviceSend),
    ToUser(CoordinatorToUserMessage),
}

#[derive(Clone, Debug)]
pub struct CoordinatorToDeviceSend {
    pub destination: Option<DeviceId>,
    pub message: CoordinatorToDeviceMessage,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum CoordinatorToDeviceMessage {
    DoKeyGen {
        devices: BTreeSet<DeviceId>,
        threshold: usize,
    },
    FinishKeyGen {
        shares_provided: BTreeMap<DeviceId, KeyGenProvideShares>,
    },
    RequestSign {
        nonces: BTreeMap<DeviceId, (Vec<Nonce>, usize, usize)>,
        message_to_sign: String,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DeviceToCoordindatorMessage {
    Announce {
        from: DeviceId,
    },
    KeyGenProvideShares(KeyGenProvideShares),
    SignatureShare {
        signature_share: Scalar<Public, Zero>,
        new_nonces: Vec<Nonce>,
        from: DeviceId,
    },
}

pub const NONCE_BATCH_SIZE: usize = 32;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct KeyGenProvideShares {
    pub from: DeviceId,
    pub my_poly: Vec<Point>,
    pub shares: BTreeMap<DeviceId, EncryptedShare>,
    pub proof_of_possession: Signature,
    pub nonces: [Nonce; NONCE_BATCH_SIZE],
}

#[derive(Clone, Debug)]
pub enum UserToCoordinatorMessage {
    StartSign {
        message_to_sign: String,
        signing_parties: Vec<DeviceId>,
    },
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {
    Signed { signature: Signature },
    CheckKeyGen { xpub: ExtendedPubKey },
}

#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    CheckKeyGen { xpub: ExtendedPubKey },
    SignatureRequest { message_to_sign: String },
}
