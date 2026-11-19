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
    ReplenishedNonces {
        device_id: DeviceId,
    },
    Card(super::card::CardToUser),
    /// A device refused a card request outside a card flow: today only the 25-word display or
    /// check of a card-restored share, which the device never shows.
    CardRefused {
        device_id: DeviceId,
        reason: crate::message::card::CardRefusal,
    },
    /// A frame from a device in a card restore, for the restore page.
    CardRestore(super::card::CardRestoreToUser),
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
