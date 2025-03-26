use super::*;

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {
    KeyGen {
        keygen_id: KeygenId,
        inner: CoordinatorToUserKeyGenMessage,
    },
    Signing(CoordinatorToUserSigningMessage),
    EnteredKnownBackup {
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        share_index: PartyIndex,
        /// whether it was a valid backup for this key
        valid: bool,
    },
    PromptRecoverShare(Box<RecoverShare>),
    PromptRecoverPhysicalBackup(Box<PhysicalBackupPhase>),
}

impl CoordinatorToUserMessage {
    pub fn kind(&self) -> &'static str {
        use CoordinatorToUserMessage::*;
        match self {
            KeyGen { .. } => "KeyGen",
            Signing(_) => "Signing",
            EnteredKnownBackup { .. } => "EnteredKnownBackup",
            PromptRecoverShare { .. } => "PromptRecoverAccessStructure",
            PromptRecoverPhysicalBackup(_) => "PromptRecoverUnknownShare",
        }
    }
}

impl Gist for CoordinatorToUserMessage {
    fn gist(&self) -> String {
        self.kind().into()
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
