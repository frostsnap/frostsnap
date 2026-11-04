use crate::{
    nonce_stream::{NonceStreamId, NonceStreamSegment},
    DeviceId, KeyId, Kind, SignSessionId,
};
use alloc::vec::Vec;
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::frost::SignatureShare;

use super::FrostCoordinator;

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, KindDerive)]
pub enum SigningMutation {
    NewNonces {
        device_id: DeviceId,
        nonce_segment: NonceStreamSegment,
    },
    NewSigningSession(super::ActiveSignSession),
    /// Also consumes the nonces the session assigned to the device, so logs written before
    /// `ConsumeNonces` existed replay to the right nonce positions.
    SentSignReq {
        session_id: SignSessionId,
        device_id: DeviceId,
    },
    GotSignatureSharesFromDevice {
        session_id: SignSessionId,
        device_id: DeviceId,
        signature_shares: Vec<SignatureShare>,
    },
    CloseSignSession {
        session_id: SignSessionId,
        finished: Option<Vec<super::EncodedSignature>>,
    },
    ForgetFinishedSignSession {
        session_id: SignSessionId,
    },
    ConsumeNonces {
        device_id: DeviceId,
        stream_id: NonceStreamId,
        up_to_but_not_including: u32,
    },
}

impl SigningMutation {
    pub(super) fn tied_to_key(&self, coord: &FrostCoordinator) -> Option<KeyId> {
        match self {
            // Nonce usage must outlive the key or devices would be asked to sign with used nonces.
            SigningMutation::NewNonces { .. } | SigningMutation::ConsumeNonces { .. } => None,
            SigningMutation::NewSigningSession(active_sign_session) => {
                Some(active_sign_session.key_id)
            }
            SigningMutation::SentSignReq { session_id, .. }
            | SigningMutation::GotSignatureSharesFromDevice { session_id, .. }
            | SigningMutation::CloseSignSession { session_id, .. }
            | SigningMutation::ForgetFinishedSignSession { session_id } => {
                Some(coord.get_sign_session(*session_id)?.key_id())
            }
        }
    }
}
