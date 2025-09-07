use crate::{nonce_stream::NonceStreamSegment, DeviceId, Kind, SignSessionId};
use alloc::vec::Vec;
use frostsnap_macros::Kind as KindDerive;
use schnorr_fun::frost::SignatureShare;

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
