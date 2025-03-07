use crate::device::KeyPurpose;
use crate::nonce_stream::CoordNonceStreamState;
use crate::tweak::BitcoinBip32Path;
use crate::{
    nonce_stream::NonceStreamSegment, AccessStructureId, AccessStructureRef, CheckedSignTask,
    CoordShareDecryptionContrib, Gist, KeygenId, MasterAppkey, SessionHash, ShareImage,
    SignSessionId, SignTaskError, Vec,
};
use crate::{DeviceId, WireSignTask};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::String,
};
use bitcoin::address::{Address, NetworkChecked};
use core::num::NonZeroU32;
use schnorr_fun::binonce;
use schnorr_fun::frost::SignatureShare;
use schnorr_fun::frost::{chilldkg::encpedpop, PartyIndex};
use schnorr_fun::fun::prelude::*;
use schnorr_fun::fun::Point;
use schnorr_fun::Signature;
use sha2::digest::Update;
use sha2::Digest;

#[derive(Clone, Debug)]
#[must_use]
pub enum DeviceSend {
    ToUser(Box<DeviceToUserMessage>),
    ToCoordinator(Box<DeviceToCoordinatorMessage>),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum CoordinatorToDeviceMessage {
    DoKeyGen(DoKeyGen),
    FinishKeyGen {
        keygen_id: KeygenId,
        agg_input: encpedpop::AggKeygenInput,
    },
    RequestSign(RequestSign),
    OpenNonceStreams {
        streams: Vec<CoordNonceStreamState>,
    },
    DisplayBackup {
        access_structure_ref: AccessStructureRef,
        coord_share_decryption_contrib: CoordShareDecryptionContrib,
        party_index: PartyIndex,
    },
    LoadKnownBackup(Box<LoadKnownBackup>),
    VerifyAddress {
        master_appkey: MasterAppkey,
        derivation_index: u32,
    },
    RequestHeldShares,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct DoKeyGen {
    pub keygen_id: KeygenId,
    pub device_to_share_index: BTreeMap<DeviceId, NonZeroU32>,
    pub threshold: u16,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

impl DoKeyGen {
    pub fn new_with_id(
        devices: BTreeSet<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        keygen_id: KeygenId,
    ) -> Self {
        let device_to_share_index: BTreeMap<_, _> = devices
            .iter()
            .enumerate()
            .map(|(index, device_id)| {
                (
                    *device_id,
                    NonZeroU32::new(index as u32 + 1).expect("we added one"),
                )
            })
            .collect();

        Self {
            device_to_share_index,
            threshold,
            key_name,
            purpose,
            keygen_id,
        }
    }
    pub fn new(
        devices: BTreeSet<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        rng: &mut impl rand_core::RngCore, // for the keygen id
    ) -> Self {
        let mut id = [0u8; 16];
        rng.fill_bytes(&mut id[..]);

        Self::new_with_id(
            devices,
            threshold,
            key_name,
            purpose,
            KeygenId::from_bytes(id),
        )
    }

    pub fn devices(&self) -> BTreeSet<DeviceId> {
        self.device_to_share_index.keys().cloned().collect()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct LoadKnownBackup {
    pub access_structure_ref: AccessStructureRef,
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub threshold: u16,
    pub share_image: ShareImage,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct GroupSignReq<ST = WireSignTask> {
    pub parties: BTreeSet<PartyIndex>,
    pub agg_nonces: Vec<binonce::Nonce<Zero>>,
    pub sign_task: ST,
    pub access_structure_id: AccessStructureId,
}

impl<ST> GroupSignReq<ST> {
    pub fn n_signatures(&self) -> usize {
        self.agg_nonces.len()
    }
}

impl GroupSignReq<WireSignTask> {
    pub fn check(
        self,
        rootkey: Point,
        purpose: KeyPurpose,
    ) -> Result<GroupSignReq<CheckedSignTask>, SignTaskError> {
        let master_appkey = MasterAppkey::derive_from_rootkey(rootkey);

        Ok(GroupSignReq {
            parties: self.parties,
            agg_nonces: self.agg_nonces,
            sign_task: self.sign_task.check(master_appkey, purpose)?,
            access_structure_id: self.access_structure_id,
        })
    }

    pub fn session_id(&self) -> SignSessionId {
        let bytes = bincode::encode_to_vec(self, bincode::config::standard()).unwrap();
        SignSessionId(sha2::Sha256::new().chain(bytes).finalize().into())
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
            CoordinatorToDeviceMessage::OpenNonceStreams { .. } => "OpenNonceStreams",
            CoordinatorToDeviceMessage::DoKeyGen { .. } => "DoKeyGen",
            CoordinatorToDeviceMessage::FinishKeyGen { .. } => "FinishKeyGen",
            CoordinatorToDeviceMessage::RequestSign { .. } => "RequestSign",
            CoordinatorToDeviceMessage::DisplayBackup { .. } => "DisplayBackup",
            CoordinatorToDeviceMessage::LoadKnownBackup { .. } => "LoadKnownBackup",
            CoordinatorToDeviceMessage::VerifyAddress { .. } => "VerifyAddress",
            CoordinatorToDeviceMessage::RequestHeldShares => "RequestHeldShares",
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum DeviceToCoordinatorMessage {
    NonceResponse {
        segments: Vec<NonceStreamSegment>,
    },
    KeyGenResponse(KeyGenResponse),
    KeyGenAck(KeyGenAck),
    SignatureShare {
        session_id: SignSessionId,
        signature_shares: Vec<SignatureShare>,
        replenish_nonces: Option<NonceStreamSegment>,
    },
    HeldShares(Vec<HeldShare>),
    LoadKnownBackupResult {
        access_structure_ref: AccessStructureRef,
        share_index: PartyIndex,
        success: bool,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct HeldShare {
    pub access_structure_ref: AccessStructureRef,
    pub share_image: ShareImage,
    pub threshold: u16,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct KeyGenResponse {
    pub keygen_id: KeygenId,
    pub input: encpedpop::KeygenInput,
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
            KeyGenAck { .. } => "KeyGenAck",
            SignatureShare { .. } => "SignatureShare",
            LoadKnownBackupResult { .. } => "LoadKnownBackupResult",
            HeldShares(_) => "HeldShares",
        }
    }
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {
    KeyGen {
        keygen_id: KeygenId,
        inner: CoordinatorToUserKeyGenMessage,
    },
    Signing(CoordinatorToUserSigningMessage),
    EnteredKnownBackup {
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        share_index: PartyIndex,
        /// whether it was a valid backup for this key
        valid: bool,
    },
    PromptRecoverShare(Box<RecoverShare>),
}

impl CoordinatorToUserMessage {
    pub fn kind(&self) -> &'static str {
        use CoordinatorToUserMessage::*;
        match self {
            KeyGen { .. } => "KeyGen",
            Signing(_) => "Signing",
            EnteredKnownBackup { .. } => "EnteredKnownBackup",
            PromptRecoverShare { .. } => "PromptRecoverAccessStructure",
        }
    }
}

#[derive(Clone, Debug, Copy, bincode::Encode, bincode::Decode, PartialEq)]
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
    GotShare {
        session_id: SignSessionId,
        from: DeviceId,
    },
    Signed {
        session_id: SignSessionId,
        signatures: Vec<EncodedSignature>,
    },
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserKeyGenMessage {
    ReceivedShares {
        from: DeviceId,
    },
    CheckKeyGen {
        session_hash: SessionHash,
    },
    KeyGenAck {
        from: DeviceId,
        all_acks_received: bool,
    },
}

/// Messages to the user often to ask them to confirm things. Often confirmations contain what we
/// call a "phase" which is both the data that describes the action and what will be passed back
/// into the core module once the action is confirmed to make progress.
#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    CheckKeyGen {
        phase: Box<crate::device::KeyGenPhase2>,
    },
    SignatureRequest {
        phase: Box<crate::device::SignPhase1>,
    },
    DisplayBackupRequest {
        phase: Box<crate::device::BackupDisplayPhase>,
    },
    DisplayBackup {
        key_name: String,
        backup: String,
    },
    EnterBackup {
        phase: Box<crate::device::LoadKnownBackupPhase>,
    },
    VerifyAddress {
        address: Address<NetworkChecked>,
        bip32_path: BitcoinBip32Path,
    },
}

#[derive(Clone, Debug)]
pub enum TaskKind {
    KeyGen,
    Sign,
    DisplayBackup,
    CheckBackup,
    VerifyAddress,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RecoverShare {
    pub held_by: DeviceId,
    pub held_share: HeldShare,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RequestSign {
    /// Common public parts of the signing request
    pub group_sign_req: GroupSignReq,
    /// Private part of the signing request that only the device should be able to access
    pub device_sign_req: DeviceSignReq,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct DeviceSignReq {
    /// Not secret but device specific. No one needs to know this other than device.
    pub nonces: CoordNonceStreamState,
    /// the rootkey - semi secret. Should not be posted publicly. Only the device should receive this.
    pub rootkey: Point,
    /// The share decryption contribution from the coordinator.
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct KeyGenAck {
    pub ack_session_hash: SessionHash,
    pub keygen_id: KeygenId,
}

impl IntoIterator for KeyGenAck {
    type Item = DeviceSend;
    type IntoIter = core::iter::Once<DeviceSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(DeviceSend::ToCoordinator(Box::new(self.into())))
    }
}

impl From<KeyGenAck> for DeviceToCoordinatorMessage {
    fn from(value: KeyGenAck) -> Self {
        DeviceToCoordinatorMessage::KeyGenAck(value)
    }
}
