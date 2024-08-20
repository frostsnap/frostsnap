use crate::encrypted_share::EncryptedShare;
use crate::{coordinator, CheckedSignTask, FrostsnapSecretKey, Gist, KeyId, SessionHash, Vec};
use crate::{DeviceId, SignTask};
use alloc::collections::VecDeque;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use schnorr_fun::fun::{marker::*, Point, Scalar};
use schnorr_fun::{binonce::Nonce, Signature};
use sha2::digest::Update;
use sha2::Digest;

#[derive(Clone, Debug)]
#[must_use]
pub enum DeviceSend {
    ToUser(DeviceToUserMessage),
    ToCoordinator(DeviceToCoordinatorMessage),
    ToStorage(DeviceToStorageMessage),
}

#[derive(Clone, Debug)]
#[must_use]
pub enum CoordinatorSend {
    ToDevice {
        message: CoordinatorToDeviceMessage,
        destinations: BTreeSet<DeviceId>,
    },
    ToUser(CoordinatorToUserMessage),
    SigningSessionStore(coordinator::SigningSessionState),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum CoordinatorToDeviceMessage {
    DoKeyGen {
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        threshold: u16,
        key_name: String,
    },
    FinishKeyGen {
        shares_provided: BTreeMap<DeviceId, KeyGenResponse>,
    },
    RequestSign(SignRequest),
    RequestNonces,
    DisplayBackup {
        key_id: KeyId,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SignRequest {
    pub nonces: BTreeMap<Scalar<Public, NonZero>, SignRequestNonces>,
    pub sign_task: SignTask,
    pub key_id: KeyId,
}

impl SignRequest {
    pub fn session_id(&self) -> [u8; 32] {
        let bytes = bincode::encode_to_vec(self, bincode::config::standard()).unwrap();
        sha2::Sha256::new().chain(bytes).finalize().into()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SignRequestNonces {
    /// the nonces the device should sign with
    pub nonces: Vec<Nonce>,
    /// The index of the first nonce
    pub start: u64,
    /// How many nonces the coordiantor has remaining
    pub nonces_remaining: u64,
}

impl SignRequest {
    pub fn signer_indicies(&self) -> impl Iterator<Item = Scalar<Public, NonZero>> + '_ {
        self.nonces.keys().cloned()
    }

    pub fn contains_signer_index(&self, id: Scalar<Public, NonZero>) -> bool {
        self.nonces.contains_key(&id)
    }
}

impl Gist for CoordinatorToDeviceMessage {
    fn gist(&self) -> String {
        self.kind().into()
    }
}

impl CoordinatorToDeviceMessage {
    pub fn kind(&self) -> &'static str {
        match self {
            CoordinatorToDeviceMessage::RequestNonces => "RequestNonces",
            CoordinatorToDeviceMessage::DoKeyGen { .. } => "DoKeyGen",
            CoordinatorToDeviceMessage::FinishKeyGen { .. } => "FinishKeyGen",
            CoordinatorToDeviceMessage::RequestSign { .. } => "RequestSign",
            CoordinatorToDeviceMessage::DisplayBackup { .. } => "DisplayBackup",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum DeviceToCoordinatorMessage {
    NonceResponse(DeviceNonces),
    KeyGenResponse(KeyGenResponse),
    KeyGenAck(SessionHash),
    SignatureShare {
        signature_shares: Vec<Scalar<Public, Zero>>,
        new_nonces: DeviceNonces,
    },
    DisplayBackupConfirmed,
}

#[derive(
    Debug, Clone, bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize, Default,
)]
pub struct DeviceNonces {
    /// the nonce index of the first nonce in `nonces`
    pub start_index: u64,
    pub nonces: VecDeque<Nonce>,
}

impl DeviceNonces {
    pub fn replenish_start(&self) -> u64 {
        self.start_index + self.nonces.len() as u64
    }
}

impl Gist for DeviceToCoordinatorMessage {
    fn gist(&self) -> String {
        self.kind().into()
    }
}

impl DeviceToCoordinatorMessage {
    pub fn kind(&self) -> &'static str {
        use DeviceToCoordinatorMessage::*;
        match self {
            NonceResponse { .. } => "NonceResponse",
            KeyGenResponse(_) => "KeyGenProvideShares",
            KeyGenAck(_) => "KeyGenAck",
            SignatureShare { .. } => "SignatureShare",
            DisplayBackupConfirmed => "DisplayBackupConfirmed",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Eq, PartialEq)]
pub struct KeyGenResponse {
    pub my_poly: Vec<Point>,
    pub encrypted_shares: BTreeMap<DeviceId, EncryptedShare>,
    pub proof_of_possession: Signature,
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {
    KeyGen(CoordinatorToUserKeyGenMessage),
    Signing(CoordinatorToUserSigningMessage),
    DisplayBackupConfirmed { device_id: DeviceId },
}

#[derive(Clone, Debug, Copy)]
/// An encoded signature that can pass ffi boundries easily
pub struct EncodedSignature(pub [u8; 64]);

impl EncodedSignature {
    pub fn new(signature: Signature) -> Self {
        Self(signature.to_bytes())
    }

    pub fn into_decoded(self) -> Option<Signature> {
        Signature::from_bytes(self.0)
    }
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserSigningMessage {
    GotShare { from: DeviceId },
    Signed { signatures: Vec<EncodedSignature> },
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserKeyGenMessage {
    ReceivedShares { from: DeviceId },
    CheckKeyGen { session_hash: SessionHash },
    KeyGenAck { from: DeviceId },
    FinishedKey { key_id: KeyId },
}

#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    CheckKeyGen {
        key_id: KeyId,
        session_hash: SessionHash,
        key_name: String,
    },
    SignatureRequest {
        sign_task: CheckedSignTask,
        key_id: KeyId,
    },
    Canceled {
        task: TaskKind,
    },
    DisplayBackupRequest {
        key_id: KeyId,
    },
    DisplayBackup {
        key_id: KeyId,
        backup: String,
    },
}

#[derive(Clone, Debug)]
pub enum TaskKind {
    KeyGen,
    Sign,
    DisplayBackup,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum DeviceToStorageMessage {
    SaveKey(FrostsnapSecretKey),
    ExpendNonce { nonce_counter: u64 },
}
