use super::*;
use frostsnap_macros::Kind;

#[derive(Clone, Debug, Kind)]
pub enum CoordinatorToUserMessage {
    KeyGen {
        keygen_id: KeygenId,
        inner: CoordinatorToUserKeyGenMessage,
    },
    Signing(CoordinatorToUserSigningMessage),
    Restoration(super::restoration::ToUserRestoration),
}

impl Gist for CoordinatorToUserMessage {
    fn gist(&self) -> String {
        crate::Kind::kind(self).into()
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
