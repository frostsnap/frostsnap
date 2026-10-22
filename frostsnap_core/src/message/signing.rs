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

impl From<DeviceSigning> for super::DeviceToCoordinatorMessage {
    fn from(value: DeviceSigning) -> Self {
        Self::Signing(value)
    }
}
