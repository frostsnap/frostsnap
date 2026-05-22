use crate::{nonce_stream::NonceStreamSegment, DeviceId, KeyId, Kind, SignSessionId};
use alloc::vec::Vec;
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::frost::{self, SignatureShare};

use super::FrostCoordinator;

pub use super::remote_signing::{combine_signatures, CombineSignatureError, RemoteSignSessionId};

/// Binonces for a participant (local or remote).
#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct ParticipantBinonces {
    pub share_index: frost::ShareIndex,
    pub binonces: Vec<schnorr_fun::binonce::Nonce>,
}

/// Signature shares from a participant (local or remote).
#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct ParticipantSignatureShares {
    pub share_index: frost::ShareIndex,
    pub signature_shares: Vec<SignatureShare>,
}

impl crate::message::GroupSignReq {
    /// Build a `GroupSignReq` from collected participant binonces.
    pub fn from_binonces(
        sign_task: crate::WireSignTask,
        access_structure_id: crate::AccessStructureId,
        all_binonces: &[ParticipantBinonces],
    ) -> Self {
        use schnorr_fun::binonce::Nonce as Binonce;

        let n_signatures = all_binonces.first().map(|b| b.binonces.len()).unwrap_or(0);
        let agg_nonces: Vec<_> = (0..n_signatures)
            .map(|i| Binonce::aggregate(all_binonces.iter().map(|b| b.binonces[i])))
            .collect();

        Self {
            sign_task,
            parties: all_binonces.iter().map(|b| b.share_index).collect(),
            agg_nonces,
            access_structure_id,
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, KindDerive)]
pub enum SigningMutation {
    NewNonces {
        device_id: DeviceId,
        nonce_segment: NonceStreamSegment,
    },
    NewSigningSession(super::ActiveSignSession),
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
}

impl SigningMutation {
    pub fn tied_to_key(&self, coord: &FrostCoordinator) -> Option<KeyId> {
        match self {
            SigningMutation::NewNonces { .. } => None,
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
