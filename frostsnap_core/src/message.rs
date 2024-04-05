use crate::encrypted_share::EncryptedShare;
use crate::xpub::TweakableKey;
use crate::CoordinatorFrostKey;
use crate::FrostsnapSecretKey;
use crate::Gist;
use crate::KeyId;
use crate::SessionHash;
use crate::SigningSessionState;
use crate::Vec;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use schnorr_fun::fun::marker::NonZero;
use schnorr_fun::fun::marker::Public;
use schnorr_fun::fun::marker::Zero;
use schnorr_fun::fun::Point;
use schnorr_fun::fun::Scalar;
use schnorr_fun::musig::Nonce;
use schnorr_fun::Message;
use schnorr_fun::Schnorr;
use schnorr_fun::Signature;

use crate::DeviceId;

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
    ToStorage(CoordinatorToStorageMessage),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum CoordinatorToDeviceMessage {
    DoKeyGen {
        device_to_share_index: BTreeMap<DeviceId, Scalar<Public, NonZero>>,
        threshold: u16,
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
pub enum CoordinatorToStorageMessage {
    NewKey(CoordinatorFrostKey),
    NoncesUsed {
        device_id: DeviceId,
        /// if nonce_counter = x, then the coordinator expects x to be the next nonce used.
        /// (anything < x has been used)
        nonce_counter: u64,
    },
    ResetNonces {
        device_id: DeviceId,
        nonces: DeviceNonces,
    },
    NewNonces {
        device_id: DeviceId,
        new_nonces: Vec<Nonce>,
    },
    StoreSigningState(SigningSessionState),
}

impl Gist for CoordinatorToStorageMessage {
    fn gist(&self) -> String {
        use CoordinatorToStorageMessage::*;
        match self {
            NoncesUsed { .. } => "NoncesUsed",
            StoreSigningState(_) => "StoreSigningState",
            ResetNonces { .. } => "ResetNonces",
            NewNonces { .. } => "NewNonces",
            NewKey(_) => "NewKey",
        }
        .into()
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
    CheckKeyGen { session_hash: SessionHash },
    SignatureRequest { sign_task: SignTask, key_id: KeyId },
    Canceled { task: TaskKind },
    DisplayBackupRequest { key_id: KeyId },
    DisplayBackup { key_id: KeyId, backup: String },
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

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum SignTask {
    Plain {
        message: Vec<u8>,
    }, // 1 nonce & sig
    Nostr {
        #[bincode(with_serde)]
        event: Box<crate::nostr::UnsignedEvent>,
    }, // 1 nonce & sig
    Transaction(TransactionSignTask),
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TransactionSignTask {
    #[bincode(with_serde)]
    pub tx_template: bitcoin::Transaction,
    pub prevouts: Vec<TxInput>,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TxInput {
    /// The txout we're spending.
    #[bincode(with_serde)]
    pub prevout: bitcoin::TxOut,
    /// The derivation path of our ket if it's ours
    pub bip32_path: Option<Vec<u32>>,
}

impl core::borrow::Borrow<bitcoin::TxOut> for TxInput {
    fn borrow(&self) -> &bitcoin::TxOut {
        &self.prevout
    }
}

// What to show on the device for signing requests
// TODO: Remove this -- the device impl should decide what to show
impl core::fmt::Display for SignTask {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignTask::Plain { message, .. } => {
                write!(f, "Plain:{}", String::from_utf8_lossy(message))
            }
            SignTask::Nostr { event, .. } => write!(f, "Nostr: {}", event.content),
            SignTask::Transaction(TransactionSignTask { tx_template, .. }) => {
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
    pub fn verify<NG>(
        &self,
        schnorr: &Schnorr<sha2::Sha256, NG>,
        root_public_key: Point,
        signatures: &[Signature],
    ) -> bool {
        self.sign_items()
            .iter()
            .enumerate()
            .all(|(i, item)| item.verify(schnorr, root_public_key, &signatures[i]))
    }

    pub fn sign_items(&self) -> Vec<SignItem> {
        match self {
            SignTask::Plain { message } => vec![SignItem {
                message: message.clone(),
                tap_tweak: false,
                bip32_path: vec![],
            }],
            SignTask::Nostr { event } => vec![SignItem {
                message: event.hash_bytes.clone(),
                tap_tweak: false,
                bip32_path: vec![],
            }],
            SignTask::Transaction(TransactionSignTask {
                tx_template,
                prevouts,
            }) => {
                use bitcoin::sighash::SighashCache;
                let mut tx_sighashes = vec![];
                let _sighash_tx = tx_template.clone();
                let schnorr_sighashty = bitcoin::sighash::TapSighashType::Default;
                for (i, _) in tx_template.input.iter().enumerate() {
                    let mut sighash_cache = SighashCache::new(&_sighash_tx);
                    let sighash = sighash_cache
                        .taproot_key_spend_signature_hash(
                            i,
                            &bitcoin::psbt::Prevouts::All(prevouts),
                            schnorr_sighashty,
                        )
                        .unwrap(); // TODO remove unwrap
                    tx_sighashes.push(sighash);
                }
                let messages = tx_sighashes
                    .into_iter()
                    .zip(prevouts.iter())
                    .filter_map(|(sighash, input)| {
                        use bitcoin::hashes::Hash;
                        let bip32_path = input.bip32_path.clone()?;
                        Some(SignItem {
                            message: sighash.as_raw_hash().to_byte_array().to_vec(),
                            bip32_path,
                            tap_tweak: true,
                        })
                    })
                    .collect();

                messages
            }
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SignItem {
    pub message: Vec<u8>,
    pub tap_tweak: bool,
    pub bip32_path: Vec<u32>,
}

impl SignItem {
    pub fn derive_key<K: TweakableKey>(&self, root_key: &K) -> K::XOnly {
        let derived_key = {
            let mut xpub = crate::xpub::Xpub::new(root_key.clone());
            xpub.derive_bip32(&self.bip32_path);
            xpub.into_key()
        };

        if self.tap_tweak {
            let tweak = bitcoin::taproot::TapTweakHash::from_key_and_tweak(
                derived_key.to_libsecp_xonly(),
                None,
            )
            .to_scalar();
            derived_key.into_xonly_with_tweak(
                Scalar::<Public, _>::from_bytes_mod_order(tweak.to_be_bytes())
                    .non_zero()
                    .expect("computationally unreachable"),
            )
        } else {
            derived_key.into_xonly()
        }
    }

    pub fn verify<NG>(
        &self,
        schnorr: &Schnorr<sha2::Sha256, NG>,
        root_public_key: Point,
        signature: &Signature,
    ) -> bool {
        // FIXME: This shouldn't be raw -- plain messages should do domain separation
        let b_message = Message::<Public>::raw(&self.message[..]);

        let derived_key = self.derive_key(&root_public_key);

        schnorr.verify(&derived_key, b_message, signature)
    }
}
