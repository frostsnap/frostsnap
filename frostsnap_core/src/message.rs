use crate::String;
use crate::Vec;
use alloc::collections::{BTreeMap, BTreeSet};
use schnorr_fun::frost::FrostKey;
use schnorr_fun::fun::marker::Normal;
use schnorr_fun::fun::marker::Public;
use schnorr_fun::fun::marker::Secret;
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

#[derive(Clone, Debug)]
pub enum CoordinatorToDeviceMessage {
    RegisterAck {},
    DoKeyGen {
        devices: BTreeSet<DeviceId>,
        threshold: usize,
    },
    FinishKeyGen {
        shares_provided: BTreeMap<DeviceId, KeyGenProvideShares>,
    },
    RequestSign {
        nonces: Vec<(DeviceId, Nonce)>,
        message_to_sign: String,
    },
}

#[derive(Clone, Debug)]
pub enum DeviceToCoordindatorMessage {
    Register {
        device_id: DeviceId,
    },
    KeyGenProvideShares(KeyGenProvideShares),
    KeyGenFinished {
        from: DeviceId,
        frost_key: FrostKey<Normal>,
        initial_nonce: Nonce,
    },
    SignatureShare {
        signature_share: Scalar<Public, Zero>,
        new_nonce: Nonce,
        from: DeviceId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyGenProvideShares {
    pub from: DeviceId,
    pub my_poly: Vec<Point>,
    pub shares: Vec<Scalar<Secret, Zero>>,
    pub proof_of_possession: Signature,
}

#[derive(Clone, Debug)]
pub enum UserToCoordinatorMessage {
    DoKeyGen {
        threshold: usize,
    },
    StartSign {
        message_to_sign: String,
        signing_parties: Vec<DeviceId>,
    },
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {
    Signed { signature: Signature },
}

#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    CheckKeyGen {
        digest: [u8; 32],
    },
    FinishedFrostKey {
        frost_key: FrostKey<Normal>,
    },
    SignatureRequest {
        message_to_sign: String,
        nonces: Vec<(DeviceId, Nonce)>,
    },
}
