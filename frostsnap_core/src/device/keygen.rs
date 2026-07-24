//! Device-side keygen: the `KeyGenPhase*` states, temporary keygen maps, and
//! the handlers for [`Keygen`] messages. Split out of `device.rs` in the same
//! shape as [`super::restoration`].

use super::*;

/// Temporary keygen state held between keygen messages. Cleared by
/// [`FrostSigner::clear_unfinished_keygens`].
#[derive(Clone, Debug, PartialEq, Default)]
pub struct State {
    tmp_keygen_phase1: BTreeMap<KeygenId, KeyGenPhase1>,
    tmp_keygen_phase2: BTreeMap<KeygenId, KeyGenPhase2>,
    tmp_keygen_pending_finalize: BTreeMap<KeygenId, (SessionHash, KeyGenPhase4)>,
}

impl State {
    pub fn clear_tmp_data(&mut self) {
        self.tmp_keygen_phase1.clear();
        self.tmp_keygen_phase2.clear();
        self.tmp_keygen_pending_finalize.clear();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenPhase1 {
    pub input_state: certpedpop::Contributor<certpedpop::ShareReceiver>,
    pub threshold: u16,
    pub key_name: String,
    pub key_purpose: KeyPurpose,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyGenPhase2 {
    pub keygen_id: KeygenId,
    share_receiver: certpedpop::SecretShareReceiver,
    key_name: String,
    key_purpose: KeyPurpose,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyGenPhase3 {
    pub keygen_id: KeygenId,
    session_hash: SessionHash,
    key_name: String,
    key_purpose: KeyPurpose,
    n_receivers: u16,
    shared_key: SharedKey,
    secret_share: PairedSecretShare,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyGenPhase4 {
    pub(in crate::device) key_name: String,
    pub(in crate::device) key_purpose: KeyPurpose,
    pub(in crate::device) access_structure_ref: AccessStructureRef,
    pub(in crate::device) access_structure_kind: AccessStructureKind,
    pub(in crate::device) encrypted_secret_share: EncryptedSecretShare,
    pub(in crate::device) threshold: u16,
}

impl KeyGenPhase2 {
    pub fn key_name(&self) -> &str {
        self.key_name.as_str()
    }
}

impl KeyGenPhase3 {
    pub fn key_name(&self) -> &str {
        self.key_name.as_str()
    }
    pub fn t_of_n(&self) -> (u16, u16) {
        let threshold = u16::try_from(self.shared_key.threshold()).expect("threshold fits in u16");
        (threshold, self.n_receivers)
    }
    pub fn session_hash(&self) -> SessionHash {
        self.session_hash
    }
}

impl<S: NonceStreamSlot + core::fmt::Debug> FrostSigner<S> {
    /// Never inlined: the keygen machinery dominates the stack, and keeping it
    /// out of `recv_coordinator_message`'s frame keeps that frame within the
    /// device's stack budget.
    #[inline(never)]
    pub fn recv_keygen_message(
        &mut self,
        keygen_msg: crate::message::Keygen,
        message: &CoordinatorToDeviceMessage,
        rng: &mut impl rand_core::RngCore,
    ) -> MessageResult<Vec<DeviceSend>> {
        match keygen_msg {
            self::Keygen::Begin(begin) => self.keygen_begin(begin, message, rng),
            self::Keygen::CertifyPlease {
                keygen_id,
                agg_input,
            } => self.keygen_certify_please(keygen_id, agg_input, message),
            self::Keygen::Check {
                keygen_id,
                certificate,
            } => self.keygen_check(keygen_id, certificate, message),
            self::Keygen::Finalize { keygen_id } => self.keygen_finalize(keygen_id, message),
        }
    }

    #[inline(never)]
    fn keygen_begin(
        &mut self,
        begin: crate::message::keygen::Begin,
        message: &CoordinatorToDeviceMessage,
        rng: &mut impl rand_core::RngCore,
    ) -> MessageResult<Vec<DeviceSend>> {
        let my_slot = begin
            .devices
            .iter()
            .position(|d| d == &self.device_id())
            .ok_or_else(|| {
                Error::signer_invalid_message(
                    message,
                    format!(
                        "my device id {} was not part of the keygen",
                        self.device_id()
                    ),
                )
            })?;
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();

        let receiver_keys: Vec<Point> = begin.devices.iter().map(|d| d.pubkey()).collect();

        let (input_state, keygen_input) =
            certpedpop::Contributor::<certpedpop::ShareReceiver>::gen_keygen_input(
                &schnorr,
                begin.threshold as u32,
                &begin.coordinator_public_keys,
                &receiver_keys,
                my_slot as u32,
                rng,
            )
            .map_err(|e| {
                Error::signer_invalid_message(message, format!("invalid keygen begin: {e}"))
            })?;
        self.keygen.tmp_keygen_phase1.insert(
            begin.keygen_id,
            KeyGenPhase1 {
                input_state,
                threshold: begin.threshold,
                key_name: begin.key_name.clone(),
                key_purpose: begin.purpose,
            },
        );
        Ok(vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::KeyGen(crate::message::keygen::DeviceKeygen::Response(
                KeyGenResponse {
                    keygen_id: begin.keygen_id,
                    input: Box::new(keygen_input),
                },
            )),
        ))])
    }

    #[inline(never)]
    fn keygen_certify_please(
        &mut self,
        keygen_id: KeygenId,
        agg_input: certpedpop::AggKeygenInput,
        message: &CoordinatorToDeviceMessage,
    ) -> MessageResult<Vec<DeviceSend>> {
        let cert_scheme = certpedpop::vrf_cert::VrfCertScheme::<Sha256>::new(
            crate::message::keygen::VRF_CERT_SCHEME_ID,
        );
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
        let phase1 = self
            .keygen
            .tmp_keygen_phase1
            .remove(&keygen_id)
            .ok_or_else(|| {
                Error::signer_invalid_message(message, "no keygen state for provided keygen_id")
            })?;

        let (share_receiver, vrf_cert) = phase1
            .input_state
            .verify_agg_input(&schnorr, &cert_scheme, agg_input, self.keypair())
            .map_err(|e| {
                Error::signer_invalid_message(
                    message,
                    format!("Failed to verify and receive share: {e}"),
                )
            })?;

        // XXX: We check the fingerprint so that a (mildly) malicious
        // coordinator cannot create key generations without the
        // fingerprint.
        if share_receiver
            .verified_agg_input()
            .shared_key()
            .check_fingerprint::<sha2::Sha256>(self.keygen_fingerprint)
            .is_none()
        {
            return Err(Error::signer_invalid_message(
                message,
                "key generation did not match the fingerprint",
            ));
        }

        self.keygen.tmp_keygen_phase2.insert(
            keygen_id,
            KeyGenPhase2 {
                keygen_id,
                share_receiver,
                key_name: phase1.key_name.clone(),
                key_purpose: phase1.key_purpose,
            },
        );
        Ok(vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::KeyGen(crate::message::keygen::DeviceKeygen::Certify {
                keygen_id,
                vrf_cert,
            }),
        ))])
    }

    #[inline(never)]
    fn keygen_check(
        &mut self,
        keygen_id: KeygenId,
        certificate: BTreeMap<Point, certpedpop::vrf_cert::CertVrfProof>,
        message: &CoordinatorToDeviceMessage,
    ) -> MessageResult<Vec<DeviceSend>> {
        let phase2 = self
            .keygen
            .tmp_keygen_phase2
            .remove(&keygen_id)
            .ok_or_else(|| {
                Error::signer_invalid_message(message, "no keygen state for provided keygen_id")
            })?;

        let cert_scheme = certpedpop::vrf_cert::VrfCertScheme::<Sha256>::new(
            crate::message::keygen::VRF_CERT_SCHEME_ID,
        );

        let (certified_keygen, secret_share) = phase2
            .share_receiver
            .finalize(&cert_scheme, certificate)
            .map_err(|e| {
                Error::signer_invalid_message(message, format!("certification failed: {e}"))
            })?;

        let session_hash = SessionHash::from_certified_keygen(&certified_keygen);
        let agg_input = certified_keygen.verified_agg_input();

        let phase3 = KeyGenPhase3 {
            keygen_id,
            n_receivers: u16::try_from(agg_input.n_receivers()).expect("n_receivers fits in u16"),
            key_name: phase2.key_name,
            key_purpose: phase2.key_purpose,
            shared_key: agg_input.shared_key(),
            secret_share,
            session_hash,
        };

        Ok(vec![DeviceSend::ToUser(Box::new(
            DeviceToUserMessage::CheckKeyGen {
                phase: Box::new(phase3),
            },
        ))])
    }

    #[inline(never)]
    fn keygen_finalize(
        &mut self,
        keygen_id: KeygenId,
        message: &CoordinatorToDeviceMessage,
    ) -> MessageResult<Vec<DeviceSend>> {
        let (_session_hash, keygen_pending_finalize) = self
            .keygen
            .tmp_keygen_pending_finalize
            .remove(&keygen_id)
            .ok_or(Error::signer_invalid_message(
                message,
                format!("device doesn't have keygen for {keygen_id}"),
            ))?;
        let key_name = keygen_pending_finalize.key_name.clone();
        self.save_complete_share(keygen_pending_finalize);

        Ok(vec![DeviceSend::ToUser(Box::new(
            DeviceToUserMessage::FinalizeKeyGen { key_name },
        ))])
    }

    pub fn keygen_ack(
        &mut self,
        phase: KeyGenPhase3,
        symm_key_gen: &mut impl DeviceSecretDerivation,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<KeyGenAck, ActionError> {
        let secret_share = phase.secret_share;
        let key_name = phase.key_name;
        let rootkey = secret_share.public_key();
        let key_id = KeyId::from_rootkey(rootkey);
        let root_shared_key = Xpub::from_rootkey(phase.shared_key);
        let app_shared_key = root_shared_key.rootkey_to_master_appkey();
        let access_structure_id =
            AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());

        // SHARE ENCRYPTION NOTE 1: We make the device gnerate the encryption key for the share right after keygen rather
        // than letting the coordinator send it to the device to protect against malicious
        // coordinators. A coordinator could provide garbage for example and then the device would
        // never be able to decrypt its share again.
        let decryption_share_contrib = CoordShareDecryptionContrib::for_master_share(
            self.device_id(),
            secret_share.index(),
            &root_shared_key.key,
        );

        let threshold = app_shared_key
            .key
            .threshold()
            .try_into()
            .expect("threshold was too large");

        let access_structure_ref = AccessStructureRef {
            key_id,
            access_structure_id,
        };

        let encrypted_secret_share = EncryptedSecretShare::encrypt(
            *secret_share.secret_share(),
            access_structure_ref,
            decryption_share_contrib,
            symm_key_gen,
            rng,
        );

        self.keygen.tmp_keygen_pending_finalize.insert(
            phase.keygen_id,
            (
                phase.session_hash,
                KeyGenPhase4 {
                    key_name,
                    key_purpose: phase.key_purpose,
                    access_structure_ref,
                    access_structure_kind: AccessStructureKind::Master,
                    threshold,
                    encrypted_secret_share,
                },
            ),
        );

        Ok(KeyGenAck {
            ack_session_hash: phase.session_hash,
            keygen_id: phase.keygen_id,
        })
    }
}
