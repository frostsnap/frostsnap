use crate::encrypted_share::EncryptedShare;
use crate::CoordinatorFrostKey;
use crate::Vec;
use crate::NONCE_BATCH_SIZE;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
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
    ToStorage(CoordinatorToStorageMessage),
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
        message_to_sign: frostsnap_ext::sign_messages::RequestSignMessage,
        tap_tweak: bool,
    },
}

impl CoordinatorToDeviceMessage {
    pub fn default_destinations(&self) -> BTreeSet<DeviceId> {
        match self {
            CoordinatorToDeviceMessage::DoKeyGen { devices, .. } => devices.clone(),
            CoordinatorToDeviceMessage::FinishKeyGen { shares_provided } => {
                shares_provided.keys().cloned().collect()
            }
            CoordinatorToDeviceMessage::RequestSign { nonces, .. } => {
                nonces.keys().cloned().collect()
            }
        }
    }
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

#[derive(Clone, Debug)]
pub enum CoordinatorToStorageMessage {
    UpdateState(CoordinatorFrostKey),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DeviceToCoordindatorMessage {
    pub from: DeviceId,
    pub body: DeviceToCoordinatorBody,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum DeviceToCoordinatorBody {
    KeyGenResponse(KeyGenResponse),
    SignatureShare {
        signature_shares: Vec<Scalar<Public, Zero>>,
        new_nonces: Vec<Nonce>,
    },
}

impl DeviceToCoordinatorBody {
    pub fn kind(&self) -> &'static str {
        match self {
            DeviceToCoordinatorBody::KeyGenResponse(_) => "KeyGenProvideShares",
            DeviceToCoordinatorBody::SignatureShare { .. } => "SignatureShare",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct KeyGenProvideShares {
    pub my_poly: Vec<Point>,
    pub shares: BTreeMap<DeviceId, EncryptedShare>,
    pub proof_of_possession: Signature,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub struct KeyGenResponse {
    pub shares: KeyGenProvideShares,
    pub nonces: [Nonce; NONCE_BATCH_SIZE],
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {
    Signed { signatures: Vec<Signature> },
    CheckKeyGen { xpub: String },
}

#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    CheckKeyGen {
        xpub: String,
    },
    SignatureRequest {
        message_to_sign: frostsnap_ext::sign_messages::RequestSignMessage,
        tap_tweak: bool,
    },
}

#[derive(Clone, Debug)]
pub enum DeviceToStorageMessage {
    SaveKey,
    ExpendNonce,
}
