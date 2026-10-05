//! Whether each connected device is genuine Frostsnap hardware.
//!
//! A device makes two attestations. In its factory attestation, its DS key, certified by the
//! factory, signs an [`AttestedDevice`] (its device id and certificate) under a challenge from this
//! coordinator. In its identity attestation, its device-id key signs a fresh challenge together
//! with that [`AttestedDevice`]'s digest. We assume the device-id key cannot be extracted (it is
//! derived from an eFuse key).
//!
//! The factory attestation is kept and re-checked on every launch, so it is asked for once per
//! coordinator and again only when the device's attested data changes, not every connection: a DS
//! signature costs about seven times a Schnorr signature on a daisy chain. Each connection only
//! asks for an identity attestation.
//!
//! The two are bound because a device's id key only signs over the digest of its own
//! [`AttestedDevice`], which includes its own certificate. A cracked device can make a factory
//! attestation claiming another device's id, but the real device will not identity-attest over
//! that attestation's digest, so a relayed identity attestation cannot pair with another device's
//! certificate. Relaying a challenge to the real device only proves that device's key is present.
//!
//! Cosmetic: nothing waits on it and a failed attestation is ignored.

use crate::firmware::FirmwareVersion;
use crate::frostsnap_persist::{FactoryAttestation, GenuineCerts};
use crate::persist::Persisted;
use frostsnap_comms::genuine_certificate::{CertificateBody, GenuineError};
use frostsnap_comms::genuine_check::{
    self, verify_factory_attestation, verify_identity_attestation,
};
use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, GenuineChallenge};
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use frostsnap_core::DeviceId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{event, Level};

#[derive(Debug, Clone, PartialEq)]
pub enum GenuineStatus {
    Unattested {
        firmware_supports_check: bool,
    },
    Attested {
        certificate: Box<CertificateBody>,
        firmware_supports_check: bool,
    },
    Genuine {
        certificate: Box<CertificateBody>,
    },
}

struct Session {
    firmware_supports_check: bool,
    proven: bool,
    outstanding: Option<genuine_check::CoordinatorMessage>,
}

pub struct GenuineCheck {
    factory_key: Point<EvenY>,
    certs: Arc<Mutex<Persisted<GenuineCerts>>>,
    db: Arc<Mutex<rusqlite::Connection>>,
    sessions: HashMap<DeviceId, Session>,
    changes: Vec<(DeviceId, GenuineStatus)>,
}

impl GenuineCheck {
    /// `certs` must have been loaded against `factory_key` from `db`.
    pub fn new(
        factory_key: Point<EvenY>,
        certs: Arc<Mutex<Persisted<GenuineCerts>>>,
        db: Arc<Mutex<rusqlite::Connection>>,
    ) -> Self {
        Self {
            factory_key,
            certs,
            db,
            sessions: Default::default(),
            changes: Default::default(),
        }
    }

    /// Call on every announce. Each one is a new connection that has to prove itself again.
    pub fn connected(
        &mut self,
        id: DeviceId,
        firmware: FirmwareVersion,
    ) -> Option<CoordinatorSendMessage> {
        let firmware_supports_check = firmware.features().genuine_check;
        let request = firmware_supports_check.then(|| {
            let challenge = GenuineChallenge::random(&mut rand::thread_rng());
            match self.certs.lock().unwrap().attested_digest(id) {
                Some(attested_digest) => {
                    genuine_check::CoordinatorMessage::RequestIdentityAttestation {
                        challenge,
                        attested_digest,
                    }
                }
                None => genuine_check::CoordinatorMessage::RequestFactoryAttestation { challenge },
            }
        });
        self.sessions.insert(
            id,
            Session {
                firmware_supports_check,
                proven: false,
                outstanding: request.clone(),
            },
        );
        self.push_status(id);
        request.map(|request| request_message(id, request))
    }

    pub fn disconnected(&mut self, id: DeviceId) {
        self.sessions.remove(&id);
    }

    /// `from` must be the id of the device the message came from, not one from the message body.
    pub fn recv(
        &mut self,
        from: DeviceId,
        message: genuine_check::DeviceMessage,
    ) -> Option<CoordinatorSendMessage> {
        let session = self.sessions.get_mut(&from)?;
        match (&session.outstanding, message) {
            (
                Some(request),
                genuine_check::DeviceMessage::FactoryAttestation {
                    attested,
                    ds_signature,
                },
            ) => {
                let challenge = request.challenge();
                match verify_factory_attestation(
                    &attested,
                    self.factory_key,
                    challenge,
                    from,
                    &ds_signature,
                ) {
                    Ok(body) => {
                        let request =
                            genuine_check::CoordinatorMessage::RequestIdentityAttestation {
                                challenge: GenuineChallenge::random(&mut rand::thread_rng()),
                                attested_digest: attested.digest(),
                            };
                        session.outstanding = Some(request.clone());
                        let attestation = FactoryAttestation {
                            attested: *attested,
                            ds_signature,
                            challenge,
                        };
                        let mut db = self.db.lock().unwrap();
                        let persisted =
                            self.certs
                                .lock()
                                .unwrap()
                                .mutate2(&mut *db, |certs, update| {
                                    certs.insert(attestation, body, update);
                                    Ok(())
                                });
                        drop(db);
                        if let Err(error) = persisted {
                            event!(
                                Level::ERROR,
                                device = from.to_string(),
                                error = error.to_string(),
                                "failed to persist a genuine factory attestation"
                            );
                        }
                        self.push_status(from);
                        Some(request_message(from, request))
                    }
                    Err(error) => {
                        ignore(from, error);
                        None
                    }
                }
            }
            (
                Some(genuine_check::CoordinatorMessage::RequestIdentityAttestation {
                    challenge,
                    attested_digest,
                }),
                genuine_check::DeviceMessage::IdentityAttestation { signature },
            ) => {
                match verify_identity_attestation(from, *challenge, *attested_digest, &signature) {
                    Ok(()) => {
                        session.outstanding = None;
                        session.proven = true;
                        self.push_status(from);
                    }
                    Err(error) => ignore(from, error),
                }
                None
            }
            _ => {
                event!(
                    Level::WARN,
                    device = from.to_string(),
                    "ignoring a genuine check response we did not ask for"
                );
                None
            }
        }
    }

    pub fn status(&self, id: DeviceId) -> Option<GenuineStatus> {
        let session = self.sessions.get(&id)?;
        let certificate = self.certs.lock().unwrap().get(id).cloned().map(Box::new);
        Some(match (certificate, session.proven) {
            (Some(certificate), true) => GenuineStatus::Genuine { certificate },
            (Some(certificate), false) => GenuineStatus::Attested {
                certificate,
                firmware_supports_check: session.firmware_supports_check,
            },
            (None, _) => GenuineStatus::Unattested {
                firmware_supports_check: session.firmware_supports_check,
            },
        })
    }

    pub fn take_changes(&mut self) -> Vec<(DeviceId, GenuineStatus)> {
        core::mem::take(&mut self.changes)
    }

    fn push_status(&mut self, id: DeviceId) {
        if let Some(status) = self.status(id) {
            self.changes.push((id, status));
        }
    }
}

fn request_message(
    id: DeviceId,
    request: genuine_check::CoordinatorMessage,
) -> CoordinatorSendMessage {
    CoordinatorSendMessage::to(id, CoordinatorSendBody::GenuineCheck(request))
}

fn ignore(from: DeviceId, error: GenuineError) {
    event!(
        Level::WARN,
        device = from.to_string(),
        error = error.to_string(),
        "ignoring a genuine check response that did not verify"
    );
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::persist::{BincodeWrapper, Persist};
    use frostsnap_comms::factory::DS_KEY_SIZE_BITS;
    use frostsnap_comms::genuine_certificate::{
        sign_certificate, CaseColor, Certificate, DsSignature,
    };
    use frostsnap_comms::genuine_check::{
        sign_identity_attestation, AttestedDevice, AttestedDeviceDigest, DsSignedMessage,
    };
    use frostsnap_comms::Sha256Digest;
    use frostsnap_core::schnorr_fun::{
        self,
        fun::{KeyPair, Scalar},
    };
    use frostsnap_core::sha2::{Digest, Sha256};
    use rand::{rngs::StdRng, SeedableRng};
    use rsa::{pkcs1::EncodeRsaPublicKey, Pkcs1v15Sign, RsaPrivateKey};
    use rusqlite::params;
    use std::sync::LazyLock;

    static FACTORY: LazyLock<KeyPair<EvenY>> =
        LazyLock::new(|| KeyPair::new_xonly(Scalar::random(&mut StdRng::seed_from_u64(1))));
    static DS_KEY: LazyLock<RsaPrivateKey> = LazyLock::new(|| {
        RsaPrivateKey::new(&mut StdRng::seed_from_u64(2), DS_KEY_SIZE_BITS).unwrap()
    });
    static OTHER_DS_KEY: LazyLock<RsaPrivateKey> = LazyLock::new(|| {
        RsaPrivateKey::new(&mut StdRng::seed_from_u64(3), DS_KEY_SIZE_BITS).unwrap()
    });

    fn supported() -> FirmwareVersion {
        FirmwareVersion::new(Sha256Digest([0xab; 32]))
    }

    struct TestDevice {
        keypair: KeyPair,
        id: DeviceId,
        ds_key: &'static RsaPrivateKey,
        certificate: Certificate,
    }

    impl TestDevice {
        fn new(seed: u64) -> Self {
            Self::certified(seed, &DS_KEY, CaseColor::Orange)
        }

        fn certified(seed: u64, ds_key: &'static RsaPrivateKey, case_color: CaseColor) -> Self {
            let keypair = KeyPair::new(Scalar::random(&mut StdRng::seed_from_u64(seed)));
            let certificate = sign_certificate(
                schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                ds_key.to_public_key().to_pkcs1_der().unwrap().to_vec(),
                case_color,
                "2.7-1625".to_string(),
                "220825002".to_string(),
                1971,
                *FACTORY,
            );
            Self {
                id: DeviceId::new(keypair.public_key()),
                keypair,
                ds_key,
                certificate,
            }
        }

        fn attested(&self) -> AttestedDevice {
            AttestedDevice::V0 {
                device_id: self.id,
                certificate: self.certificate.clone(),
            }
        }

        /// This device's DS key signing any `attested`, as a cracked device could.
        fn factory_attest(
            &self,
            challenge: GenuineChallenge,
            attested: AttestedDevice,
        ) -> genuine_check::DeviceMessage {
            let signed = DsSignedMessage::GenuineAttestationV0 {
                challenge,
                attested: attested.clone(),
            };
            let digest: [u8; 32] = Sha256::digest(signed.to_signing_bytes()).into();
            let ds_signature = self
                .ds_key
                .sign(Pkcs1v15Sign::new::<Sha256>(), &digest)
                .unwrap()
                .try_into()
                .unwrap();
            genuine_check::DeviceMessage::FactoryAttestation {
                attested: Box::new(attested),
                ds_signature: Box::new(DsSignature(ds_signature)),
            }
        }

        fn identity_attest(
            &self,
            challenge: GenuineChallenge,
            attested_digest: AttestedDeviceDigest,
        ) -> genuine_check::DeviceMessage {
            genuine_check::DeviceMessage::IdentityAttestation {
                signature: sign_identity_attestation(
                    &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                    &self.keypair,
                    challenge,
                    attested_digest,
                ),
            }
        }

        /// What honest firmware answers.
        fn answer(
            &self,
            request: &genuine_check::CoordinatorMessage,
        ) -> genuine_check::DeviceMessage {
            let attested = self.attested();
            match request.device_action(attested.digest()) {
                genuine_check::DeviceAction::FactoryAttest(challenge) => {
                    self.factory_attest(challenge, attested)
                }
                genuine_check::DeviceAction::IdentityAttest(challenge) => {
                    self.identity_attest(challenge, attested.digest())
                }
            }
        }
    }

    fn genuine_request(message: &CoordinatorSendMessage) -> &genuine_check::CoordinatorMessage {
        match &message.message_body {
            CoordinatorSendBody::GenuineCheck(request) => request,
            other => panic!("not a genuine check request: {other:?}"),
        }
    }

    fn new_check() -> GenuineCheck {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let certs = Persisted::<GenuineCerts>::new(&mut db, FACTORY.public_key()).unwrap();
        GenuineCheck::new(
            FACTORY.public_key(),
            Arc::new(Mutex::new(certs)),
            Arc::new(Mutex::new(db)),
        )
    }

    fn is_genuine(check: &GenuineCheck, id: DeviceId) -> bool {
        matches!(check.status(id), Some(GenuineStatus::Genuine { .. }))
    }

    fn unsupported() -> FirmwareVersion {
        FirmwareVersion {
            digest: Sha256Digest([0xcd; 32]),
            version: Some(crate::firmware::VersionNumber::new(0, 4, 0)),
        }
    }

    /// Returns the requests the device was sent.
    fn prove(
        check: &mut GenuineCheck,
        device: &TestDevice,
    ) -> Vec<genuine_check::CoordinatorMessage> {
        let mut sent = vec![];
        let mut message = check.connected(device.id, supported());
        while let Some(request) = message.as_ref().map(genuine_request).cloned() {
            message = check.recv(device.id, device.answer(&request));
            sent.push(request);
        }
        sent
    }

    fn attested_check(device: &TestDevice) -> GenuineCheck {
        let mut check = new_check();
        prove(&mut check, device);
        check.disconnected(device.id);
        check.take_changes();
        check
    }

    fn stored_rows(check: &GenuineCheck) -> Vec<(DeviceId, Vec<u8>)> {
        let db = check.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT id, attestation FROM fs_genuine_attested_devices ORDER BY id")
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn reload(check: &GenuineCheck) -> anyhow::Result<GenuineCerts> {
        GenuineCerts::load(&mut check.db.lock().unwrap(), FACTORY.public_key())
    }

    #[test]
    fn every_announce_is_a_new_connection_that_must_prove_itself() {
        let device = TestDevice::new(10);
        let mut check = new_check();
        prove(&mut check, &device);
        assert!(is_genuine(&check, device.id));

        check.take_changes();
        let request = check
            .connected(device.id, supported())
            .expect("the new connection is challenged");
        assert!(matches!(
            check.take_changes().as_slice(),
            [(_, GenuineStatus::Attested { .. })]
        ));
        assert!(!is_genuine(&check, device.id));

        check.recv(device.id, device.answer(genuine_request(&request)));
        assert!(is_genuine(&check, device.id));
    }

    #[test]
    fn a_reply_to_the_previous_announces_challenge_does_not_strand_the_device() {
        let device = TestDevice::new(13);
        let mut check = attested_check(&device);

        let previous = check.connected(device.id, supported()).unwrap();
        let current = check.connected(device.id, supported()).unwrap();
        check.recv(device.id, device.answer(genuine_request(&previous)));
        assert!(!is_genuine(&check, device.id));

        check.recv(device.id, device.answer(genuine_request(&current)));
        assert!(is_genuine(&check, device.id));
    }

    #[test]
    fn a_malformed_stored_row_does_not_stop_the_load() -> anyhow::Result<()> {
        let device = TestDevice::new(11);
        let other = TestDevice::new(12);
        let mut check = new_check();
        prove(&mut check, &device);
        check.db.lock().unwrap().execute(
            "INSERT INTO fs_genuine_attested_devices (id, attestation) \
             VALUES ('not an id', x'00'), (?1, x'00')",
            params![other.id],
        )?;

        let loaded = reload(&check)?;
        assert!(loaded.get(device.id).is_some());
        assert!(loaded.get(other.id).is_none());
        Ok(())
    }

    #[test]
    fn a_device_with_no_certificate_goes_1_2_3_in_two_queries_and_its_certificate_is_persisted(
    ) -> anyhow::Result<()> {
        let device = TestDevice::new(20);
        let mut check = new_check();

        let attest = check.connected(device.id, supported()).unwrap();
        assert!(matches!(
            genuine_request(&attest),
            genuine_check::CoordinatorMessage::RequestFactoryAttestation { .. }
        ));
        assert_eq!(
            check.status(device.id),
            Some(GenuineStatus::Unattested {
                firmware_supports_check: true
            })
        );

        let challenge = check
            .recv(device.id, device.answer(genuine_request(&attest)))
            .unwrap();
        assert!(matches!(
            genuine_request(&challenge),
            genuine_check::CoordinatorMessage::RequestIdentityAttestation { attested_digest, .. }
                if *attested_digest == device.attested().digest()
        ));
        assert!(matches!(
            check.status(device.id),
            Some(GenuineStatus::Attested { .. })
        ));

        assert!(check
            .recv(device.id, device.answer(genuine_request(&challenge)))
            .is_none());
        assert!(is_genuine(&check, device.id));
        assert_eq!(check.take_changes().len(), 3);

        let reloaded = reload(&check)?;
        assert_eq!(
            reloaded.get(device.id).unwrap().case_color(),
            CaseColor::Orange
        );
        assert_eq!(
            reloaded.attested_digest(device.id),
            Some(device.attested().digest())
        );
        Ok(())
    }

    #[test]
    fn a_device_with_a_certificate_on_file_goes_2_3_in_one_query_with_no_ds_signature() {
        let device = TestDevice::new(21);
        let mut check = attested_check(&device);
        let stored = stored_rows(&check);

        let sent = prove(&mut check, &device);
        assert!(matches!(
            sent.as_slice(),
            [genuine_check::CoordinatorMessage::RequestIdentityAttestation { .. }]
        ));
        assert!(is_genuine(&check, device.id));
        assert_eq!(stored_rows(&check), stored);
    }

    #[test]
    fn a_device_whose_attested_data_changed_attests_again_then_proves_itself() -> anyhow::Result<()>
    {
        let before = TestDevice::new(35);
        let mut check = attested_check(&before);
        let device = TestDevice::certified(35, &DS_KEY, CaseColor::Red);
        assert_eq!(device.id, before.id);

        let stale = check.connected(device.id, supported()).unwrap();
        assert!(matches!(
            genuine_request(&stale),
            genuine_check::CoordinatorMessage::RequestIdentityAttestation { attested_digest, .. }
                if *attested_digest == before.attested().digest()
        ));
        check.take_changes();

        let answer = device.answer(genuine_request(&stale));
        assert!(matches!(
            answer,
            genuine_check::DeviceMessage::FactoryAttestation { .. }
        ));
        let fresh = check.recv(device.id, answer).unwrap();
        assert!(matches!(
            genuine_request(&fresh),
            genuine_check::CoordinatorMessage::RequestIdentityAttestation { challenge, attested_digest }
                if *attested_digest == device.attested().digest()
                    && *challenge != genuine_request(&stale).challenge()
        ));
        assert!(matches!(
            check.take_changes().as_slice(),
            [(_, GenuineStatus::Attested { certificate, .. })]
                if certificate.case_color() == CaseColor::Red
        ));
        assert_eq!(
            reload(&check)?.attested_digest(device.id),
            Some(device.attested().digest())
        );

        assert!(check
            .recv(device.id, device.answer(genuine_request(&fresh)))
            .is_none());
        assert!(matches!(
            check.status(device.id),
            Some(GenuineStatus::Genuine { certificate })
                if certificate.case_color() == CaseColor::Red
        ));
        Ok(())
    }

    #[test]
    fn a_relayed_identity_attestation_never_makes_another_devices_certificate_genuine() {
        let victim = TestDevice::new(36);
        let attacker = TestDevice::certified(37, &OTHER_DS_KEY, CaseColor::Red);
        let mut check = new_check();

        let attest = check.connected(victim.id, supported()).unwrap();
        let forged = AttestedDevice::V0 {
            device_id: victim.id,
            certificate: attacker.certificate.clone(),
        };
        let challenge_for_forged = check
            .recv(
                victim.id,
                attacker.factory_attest(genuine_request(&attest).challenge(), forged.clone()),
            )
            .expect("the attacker's DS key really did sign it");
        assert!(matches!(
            genuine_request(&challenge_for_forged),
            genuine_check::CoordinatorMessage::RequestIdentityAttestation { attested_digest, .. }
                if *attested_digest == forged.digest()
        ));

        let relayed = victim.answer(genuine_request(&challenge_for_forged));
        assert!(matches!(
            relayed,
            genuine_check::DeviceMessage::FactoryAttestation { .. }
        ));
        let challenge_for_victim = check.recv(victim.id, relayed).unwrap();
        check.recv(
            victim.id,
            victim.answer(genuine_request(&challenge_for_victim)),
        );

        let changes = check.take_changes();
        assert!(changes.iter().all(|(_, status)| !matches!(
            status,
            GenuineStatus::Genuine { certificate } if certificate.case_color() == CaseColor::Red
        )));
        assert!(matches!(
            check.status(victim.id),
            Some(GenuineStatus::Genuine { certificate })
                if certificate.case_color() == CaseColor::Orange
        ));
    }

    #[test]
    fn a_failed_factory_attestation_leaves_state_and_storage_unchanged() {
        let device = TestDevice::new(22);
        let mut check = new_check();
        let attest = check.connected(device.id, supported()).unwrap();
        check.take_changes();

        let for_another_device = device.factory_attest(
            genuine_request(&attest).challenge(),
            TestDevice::new(99).attested(),
        );
        assert!(check.recv(device.id, for_another_device).is_none());
        assert_eq!(
            check.status(device.id),
            Some(GenuineStatus::Unattested {
                firmware_supports_check: true
            })
        );
        assert!(check.take_changes().is_empty());
        assert!(stored_rows(&check).is_empty());

        check.recv(device.id, device.answer(genuine_request(&attest)));
        assert!(check.certs.lock().unwrap().get(device.id).is_some());
    }

    #[test]
    fn another_devices_factory_attestation_is_rejected() {
        let device = TestDevice::new(23);
        let impostor = TestDevice::new(24);
        let mut check = new_check();
        let attest = check.connected(impostor.id, supported()).unwrap();

        check.recv(impostor.id, device.answer(genuine_request(&attest)));
        assert!(check.certs.lock().unwrap().get(impostor.id).is_none());
        assert!(matches!(
            check.status(impostor.id),
            Some(GenuineStatus::Unattested { .. })
        ));
    }

    #[test]
    fn a_failed_identity_attestation_leaves_state_and_storage_unchanged() {
        let device = TestDevice::new(25);
        let impostor = TestDevice::new(26);
        let mut check = attested_check(&device);
        let stored = stored_rows(&check);

        let challenge = check.connected(device.id, supported()).unwrap();
        check.take_changes();
        check.recv(
            device.id,
            impostor.identity_attest(
                genuine_request(&challenge).challenge(),
                device.attested().digest(),
            ),
        );

        assert!(matches!(
            check.status(device.id),
            Some(GenuineStatus::Attested { .. })
        ));
        assert!(check.take_changes().is_empty());
        assert_eq!(stored_rows(&check), stored);
        assert!(check.certs.lock().unwrap().get(device.id).is_some());
    }

    #[test]
    fn a_disconnect_returns_a_device_to_state_2() {
        let device = TestDevice::new(27);
        let mut check = new_check();
        prove(&mut check, &device);
        assert!(is_genuine(&check, device.id));

        check.disconnected(device.id);
        assert_eq!(check.status(device.id), None);

        check.connected(device.id, supported());
        assert!(matches!(
            check.status(device.id),
            Some(GenuineStatus::Attested {
                firmware_supports_check: true,
                ..
            })
        ));
    }

    #[test]
    fn each_connection_gets_a_fresh_challenge_and_an_old_answer_does_not_verify() {
        let device = TestDevice::new(28);
        let mut check = attested_check(&device);

        let first = check.connected(device.id, supported()).unwrap();
        let old_answer = device.answer(genuine_request(&first));
        check.disconnected(device.id);

        let second = check.connected(device.id, supported()).unwrap();
        assert_ne!(
            genuine_request(&first).challenge(),
            genuine_request(&second).challenge()
        );

        check.recv(device.id, old_answer);
        assert!(!is_genuine(&check, device.id));
    }

    #[test]
    fn firmware_without_the_feature_is_never_sent_either_request() {
        let unattested = TestDevice::new(29);
        let attested = TestDevice::new(30);
        let mut check = attested_check(&attested);

        for device in [&unattested, &attested] {
            assert!(check.connected(device.id, unsupported()).is_none());
            assert!(check.connected(device.id, unsupported()).is_none());
        }
        assert_eq!(
            check.status(unattested.id),
            Some(GenuineStatus::Unattested {
                firmware_supports_check: false
            })
        );
        assert!(matches!(
            check.status(attested.id),
            Some(GenuineStatus::Attested {
                firmware_supports_check: false,
                ..
            })
        ));
    }

    #[test]
    fn an_unsolicited_response_is_ignored() {
        let device = TestDevice::new(31);
        let mut check = new_check();
        check.connected(device.id, unsupported());
        check.take_changes();

        check.recv(
            device.id,
            device.factory_attest(GenuineChallenge([0; 32]), device.attested()),
        );
        assert!(check.take_changes().is_empty());
        assert!(stored_rows(&check).is_empty());
    }

    #[test]
    fn a_factory_attestation_made_for_another_coordinators_challenge_is_rejected() {
        let device = TestDevice::new(32);
        let mut coordinator_a = new_check();
        let mut coordinator_b = new_check();

        let request_a = coordinator_a.connected(device.id, supported()).unwrap();
        let captured = device.answer(genuine_request(&request_a));

        coordinator_b.connected(device.id, supported()).unwrap();
        coordinator_b.take_changes();
        assert!(coordinator_b.recv(device.id, captured).is_none());
        assert!(coordinator_b.certs.lock().unwrap().get(device.id).is_none());
        assert!(coordinator_b.take_changes().is_empty());
        assert!(stored_rows(&coordinator_b).is_empty());
    }

    #[test]
    fn a_stored_factory_attestation_only_reloads_with_the_challenge_it_was_made_for(
    ) -> anyhow::Result<()> {
        let device = TestDevice::new(33);
        let mut check = new_check();
        prove(&mut check, &device);
        assert!(reload(&check)?.get(device.id).is_some());

        {
            let db = check.db.lock().unwrap();
            let BincodeWrapper(mut attestation) = db.query_row(
                "SELECT attestation FROM fs_genuine_attested_devices",
                [],
                |row| row.get::<_, BincodeWrapper<FactoryAttestation>>(0),
            )?;
            attestation.challenge = GenuineChallenge([0; 32]);
            db.execute(
                "UPDATE fs_genuine_attested_devices SET attestation = ?1",
                params![BincodeWrapper(attestation)],
            )?;
        }
        assert!(reload(&check)?.get(device.id).is_none());
        Ok(())
    }

    #[test]
    fn a_stored_factory_attestation_only_reloads_under_the_device_it_attests() -> anyhow::Result<()>
    {
        let device = TestDevice::new(34);
        let other = TestDevice::new(38);
        let mut check = new_check();
        prove(&mut check, &device);
        check.db.lock().unwrap().execute(
            "UPDATE fs_genuine_attested_devices SET id = ?1",
            params![other.id],
        )?;

        let reloaded = reload(&check)?;
        assert!(reloaded.get(other.id).is_none());
        assert!(reloaded.get(device.id).is_none());
        Ok(())
    }
}
