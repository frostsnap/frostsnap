use crate::device::KeyPurpose;
use crate::nonce_stream::CoordNonceStreamState;
use crate::{
    nonce_stream::NonceStreamSegment, AccessStructureId, AccessStructureRef, CheckedSignTask,
    CoordShareDecryptionContrib, Gist, KeygenId, MasterAppkey, SessionHash, ShareImage,
    SignSessionId, SignTaskError, Vec,
};
use crate::{DeviceId, Kind, RestorationId, WireSignTask};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::String,
};
use core::num::NonZeroU32;
use frostsnap_macros::Kind;
use schnorr_fun::binonce;
use schnorr_fun::frost::{chilldkg::encpedpop, PartyIndex};
use schnorr_fun::frost::{SharedKey, SignatureShare};
use schnorr_fun::fun::prelude::*;
use schnorr_fun::fun::Point;
use schnorr_fun::Signature;
use sha2::digest::Update;
use sha2::Digest;

#[derive(Clone, Debug)]
#[must_use]
pub enum DeviceSend {
    ToUser(Box<crate::device::DeviceToUserMessage>),
    ToCoordinator(Box<DeviceToCoordinatorMessage>),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
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
    #[delegate_kind]
    Restoration(CoordinatorRestoration),
    VerifyAddress {
        master_appkey: MasterAppkey,
        derivation_index: u32,
    },
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

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum CoordinatorRestoration {
    EnterPhysicalBackup {
        restoration_id: RestorationId,
    },
    SavePhysicalBackup {
        restoration_id: RestorationId,
    },
    /// Consolidate the saved secret share backup into a properly encrypted backup.
    Consolidate(Box<ConsolidateBackup>),
    DisplayBackup {
        access_structure_ref: AccessStructureRef,
        coord_share_decryption_contrib: CoordShareDecryptionContrib,
        party_index: PartyIndex,
    },
    RequestHeldShares,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct ConsolidateBackup {
    pub restoration_id: RestorationId,
    pub share_index: PartyIndex,
    pub root_shared_key: SharedKey,
    pub key_name: String,
    pub purpose: KeyPurpose,
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
        crate::Kind::kind(self).into()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
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
    #[delegate_kind]
    Restoration(DeviceRestoration),
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum DeviceRestoration {
    PhysicalLoaded(EnteredPhysicalBackup),
    PhysicalSaved(EnteredPhysicalBackup),
    FinishedConsolidation { restoration_id: RestorationId },
    HeldShares(Vec<HeldShare>),
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
        Kind::kind(self).into()
    }
}

#[derive(Clone, Copy, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct EnteredPhysicalBackup {
    pub restoration_id: RestorationId,
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
        DeviceToCoordinatorMessage::KeyGenAck(value)
    }
}
