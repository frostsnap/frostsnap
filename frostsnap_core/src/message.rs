use crate::encrypted_share::EncryptedShare;
use crate::xpub::ExtendedPubKey;
use crate::String;
use crate::Vec;
use crate::NONCE_BATCH_SIZE;

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
    ToStorage(DeviceToStorageMessage),
}

#[derive(Clone, Debug)]
pub enum CoordinatorSend {
    ToDevice(CoordinatorToDeviceMessage),
    ToUser(CoordinatorToUserMessage),
}

// #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
// pub struct CoordinatorToDeviceSend {
//     pub destination: Option<DeviceId>,
//     pub message: CoordinatorToDeviceMessage,
// }

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

impl CoordinatorToDeviceMessage {
    pub fn kind(&self) -> &'static str {
        match self {
            CoordinatorToDeviceMessage::DoKeyGen { .. } => "DoKeyGen",
            CoordinatorToDeviceMessage::FinishKeyGen { .. } => "FinishKeyGen",
            CoordinatorToDeviceMessage::RequestSign { .. } => "RequestSign",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DeviceToCoordindatorMessage {
    pub from: DeviceId,
    pub body: DeviceToCoordinatorBody,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DeviceToCoordinatorBody {
    KeyGenProvideShares(KeyGenProvideShares),
    SignatureShare {
        signature_share: Scalar<Public, Zero>,
        new_nonces: Vec<Nonce>,
    },
}

impl DeviceToCoordinatorBody {
    pub fn kind(&self) -> &'static str {
        match self {
            DeviceToCoordinatorBody::KeyGenProvideShares(_) => "KeyGenProvideShares",
            DeviceToCoordinatorBody::SignatureShare { .. } => "SignatureShare",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct KeyGenProvideShares {
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

#[derive(Clone, Debug)]
pub enum DeviceToStorageMessage {
    SaveKey,
    ExpendNonce,
}
