use crate::{
    nonce_stream::{CoordNonceStreamState, NonceStreamSegment},
    silent_payments::EcdhShare,
    AccessStructureId, CoordShareDecryptionContrib, Kind, SignSessionId, WireSignTask,
};
use alloc::{boxed::Box, vec::Vec};
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::{
    frost::SignatureShare,
    fun::{marker::*, Point, Scalar},
};

/// A request to open one or more nonce streams
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct OpenNonceStreams {
    pub streams: Vec<CoordNonceStreamState>,
}

impl OpenNonceStreams {
    /// Split into individual OpenNonceStreams messages, each with one stream
    pub fn split(self) -> Vec<OpenNonceStreams> {
        self.streams
            .into_iter()
            .map(|stream| OpenNonceStreams {
                streams: vec![stream],
            })
            .collect()
    }
}

/// Coordinator to device signing messages
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, KindDerive)]
pub enum CoordinatorSigning {
    RequestSign(Box<super::RequestSign>),
    OpenNonceStreams(OpenNonceStreams),
    RequestSpEcdh(Box<RequestSpEcdh>),
    RequestSpSign(Box<RequestSpSign>),
}

/// Device to coordinator signing messages  
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, KindDerive)]
pub enum DeviceSigning {
    NonceResponse {
        segments: Vec<NonceStreamSegment>,
    },
    SignatureShare {
        session_id: SignSessionId,
        signature_shares: Vec<SignatureShare>,
        replenish_nonces: Option<NonceStreamSegment>,
    },
    SpEcdhShares {
        session_id: SpEcdhSessionId,
        /// One share per scan key of the transaction, in the order the device was asked for.
        shares: Vec<EcdhShare>,
    },
}

/// Identifies one threshold ECDH run for a silent payment send.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, bincode::Encode, bincode::Decode,
)]
pub struct SpEcdhSessionId(pub [u8; 32]);

/// Asks a device to compute its share of the silent payment ECDH for a template with holes.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RequestSpEcdh {
    pub session_id: SpEcdhSessionId,
    pub rootkey: Point,
    pub access_structure_id: AccessStructureId,
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
    pub scan_keys: Vec<Point>,
    pub input_hash: Scalar<Public>,
    pub sign_task: WireSignTask,
}

/// Everything a device needs to check the silent payment outputs of a sign request: the public
/// polynomial the access structure is identified by, and the ECDH shares of a threshold of parties
/// for every scan key. Shares are the signing devices' own, so no party needs to trust another.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SpProofBundle {
    pub app_poly: Vec<Point<Normal, Public, Zero>>,
    pub shares: Vec<EcdhShare>,
}

/// A sign request for a transaction with silent payment outputs.
#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RequestSpSign {
    pub request_sign: super::RequestSign,
    pub sp_proofs: SpProofBundle,
}

impl From<DeviceSigning> for super::DeviceToCoordinatorMessage {
    fn from(value: DeviceSigning) -> Self {
        Self::Signing(value)
    }
}
