use crate::{
    nonce_stream::{CoordNonceStreamState, NonceStreamSegment},
    silent_payments::{EcdhShare, SpContactDetails},
    AccessStructureId, CoordShareDecryptionContrib, Kind, ShareIndex, SignSessionId,
};
use alloc::{boxed::Box, vec::Vec};
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::{
    frost::SignatureShare,
    fun::{marker::*, Point},
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
    RequestSpContactEcdh(Box<RequestSpContactEcdh>),
    RequestSpContactSign(Box<RequestSpContactSign>),
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
    SpContactEcdhShare {
        session_id: SpContactSessionId,
        share: Box<EcdhShare>,
    },
}

/// Identifies one run of adding a contact, before the wallet signs it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, bincode::Encode, bincode::Decode,
)]
pub struct SpContactSessionId(pub [u8; 32]);

/// Asks a device to confirm a contact with the user and answer with its share of the ECDH.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RequestSpContactEcdh {
    pub session_id: SpContactSessionId,
    pub rootkey: Point,
    pub access_structure_id: AccessStructureId,
    /// The share the device answers with, in case it holds more than one.
    pub share_index: ShareIndex,
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
    pub details: SpContactDetails,
}

/// What a device checks a contact's `x * B_scan` against before signing it: the public polynomial
/// the access structure is identified by, and the ECDH shares of a threshold of parties.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SpContactProofs {
    pub app_poly: Vec<Point<Normal, Public, Zero>>,
    pub shares: Vec<EcdhShare>,
}

/// A sign request for a contact, carrying the proofs the signing session itself does not keep.
#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RequestSpContactSign {
    pub request_sign: super::RequestSign,
    pub proofs: SpContactProofs,
}

impl From<DeviceSigning> for super::DeviceToCoordinatorMessage {
    fn from(value: DeviceSigning) -> Self {
        Self::Signing(value)
    }
}
