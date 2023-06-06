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
        // TODO: explain these `usize` and create a nicely documented struct which explains the
        // mechanism
        nonces: BTreeMap<DeviceId, (Vec<Nonce>, usize, usize)>,
        message_to_sign: SignTask,
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
    KeyGenProvideShares(KeyGenProvideShares),
    SignatureShare {
        signature_shares: Vec<Scalar<Public, Zero>>,
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
        message_to_sign: SignTask,
        tap_tweak: bool,
    },
}

#[derive(Clone, Debug)]
pub enum DeviceToStorageMessage {
    SaveKey,
    ExpendNonce,
}

#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd,
)]
pub enum SignTask {
    Plain(Vec<u8>),                     // 1 nonce & sig
    Nostr(crate::nostr::UnsignedEvent), // 1 nonce & sig
    Transaction {
        tx_template: bitcoin::Transaction,
        prevouts: Vec<bitcoin::TxOut>,
    }, // N nonces and sigs
}

// What to show on the device for signing requests
impl core::fmt::Display for SignTask {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignTask::Plain(message) => {
                write!(f, "Plain:{}", String::from_utf8_lossy(message))
            }
            SignTask::Nostr(event) => write!(f, "Nostr: {}", event.content),
            SignTask::Transaction { tx_template, .. } => {
                let mut lines = vec![];
                for output in &tx_template.output {
                    let address = bitcoin::Address::from_script(
                        &output.script_pubkey,
                        bitcoin::Network::Signet,
                    )
                    .expect("valid address");
                    lines.push(format!("{} to {}", output.value, address));
                }
                write!(f, "{}", lines.join("\n"))
            }
        }
    }
}

// The bytes which need to be signed
impl SignTask {
    pub fn messages_to_sign(&self) -> Vec<Vec<u8>> {
        match self {
            SignTask::Plain(message) => vec![message.to_vec()],
            SignTask::Nostr(event) => vec![event.hash_bytes.clone()],
            SignTask::Transaction {
                tx_template,
                prevouts,
            } => {
                use bitcoin::util::sighash::SighashCache;
                let mut tx_sighashes = vec![];
                let _sighash_tx = tx_template.clone();
                let schnorr_sighashty = bitcoin::SchnorrSighashType::Default;
                for (i, _) in tx_template.input.iter().enumerate() {
                    let mut sighash_cache = SighashCache::new(&_sighash_tx);
                    let sighash = sighash_cache
                        .taproot_key_spend_signature_hash(
                            i,
                            &bitcoin::psbt::Prevouts::All(&prevouts),
                            schnorr_sighashty,
                        )
                        .unwrap(); // TODO remove unwrap
                    tx_sighashes.push(sighash);
                }
                let messages = tx_sighashes
                    .into_iter()
                    .map(|sighash| sighash.to_vec())
                    .collect();

                messages
            }
        }
    }
}
