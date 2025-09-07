use crate::device::KeyPurpose;
use crate::nonce_stream::CoordNonceStreamState;
use crate::{
    nonce_stream::NonceStreamSegment, AccessStructureId, AccessStructureRef, CheckedSignTask,
    CoordShareDecryptionContrib, Gist, KeygenId, MasterAppkey, SessionHash, ShareImage,
    SignSessionId, SignTaskError, Vec,
};
use crate::{DeviceId, EnterPhysicalId, Kind, WireSignTask};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::String,
};
use frostsnap_macros::Kind;
use schnorr_fun::binonce;
use schnorr_fun::frost::{chilldkg::certpedpop, ShareIndex};
use schnorr_fun::frost::{SharedKey, SignatureShare};
use schnorr_fun::fun::prelude::*;
use schnorr_fun::fun::Point;
use schnorr_fun::Signature;
use sha2::digest::Update;
use sha2::Digest;

pub mod keygen;
pub use keygen::Keygen;

#[derive(Clone, Debug)]
#[must_use]
pub enum DeviceSend {
    ToUser(Box<crate::device::DeviceToUserMessage>),
    ToCoordinator(Box<DeviceToCoordinatorMessage>),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum CoordinatorToDeviceMessage {
    KeyGen(keygen::Keygen),
    RequestSign(Box<RequestSign>),
    OpenNonceStreams {
        streams: Vec<CoordNonceStreamState>,
    },
    #[delegate_kind]
    Restoration(CoordinatorRestoration),
    VerifyAddress {
        master_appkey: MasterAppkey,
        derivation_index: u32,
    },
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum CoordinatorRestoration {
    EnterPhysicalBackup {
        enter_physical_id: EnterPhysicalId,
    },
    SavePhysicalBackup {
        share_image: ShareImage,
        key_name: String,
        purpose: KeyPurpose,
        threshold: u16,
    },
    /// Consolidate the saved secret share backup into a properly encrypted backup.
    Consolidate(Box<ConsolidateBackup>),
    DisplayBackup {
        access_structure_ref: AccessStructureRef,
        coord_share_decryption_contrib: CoordShareDecryptionContrib,
        party_index: ShareIndex,
        root_shared_key: SharedKey,
    },
    RequestHeldShares,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct ConsolidateBackup {
    pub share_index: ShareIndex,
    pub root_shared_key: SharedKey,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct GroupSignReq<ST = WireSignTask> {
    pub parties: BTreeSet<ShareIndex>,
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
        crate::Kind::kind(self).into()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum DeviceToCoordinatorMessage {
    NonceResponse {
        segments: Vec<NonceStreamSegment>,
    },
    KeyGen(keygen::DeviceKeygen),
    SignatureShare {
        session_id: SignSessionId,
        signature_shares: Vec<SignatureShare>,
        replenish_nonces: Option<NonceStreamSegment>,
    },
    #[delegate_kind]
    Restoration(DeviceRestoration),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum DeviceRestoration {
    PhysicalEntered(EnteredPhysicalBackup),
    PhysicalSaved(ShareImage),
    FinishedConsolidation {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
    HeldShares(Vec<HeldShare>),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct HeldShare {
    pub access_structure_ref: Option<AccessStructureRef>,
    pub share_image: ShareImage,
    pub threshold: u16,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct KeyGenResponse {
    pub keygen_id: KeygenId,
    pub input: Box<certpedpop::KeygenInput>,
}

impl Gist for DeviceToCoordinatorMessage {
    fn gist(&self) -> String {
        Kind::kind(self).into()
    }
}

#[derive(Clone, Copy, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct EnteredPhysicalBackup {
    pub enter_physical_id: EnterPhysicalId,
    pub share_image: ShareImage,
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
pub enum TaskKind {
    KeyGen,
    Sign,
    DisplayBackup,
    CheckBackup,
    VerifyAddress,
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
        DeviceToCoordinatorMessage::KeyGen(keygen::DeviceKeygen::Ack(value))
    }
}
