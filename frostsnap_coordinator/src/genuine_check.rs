//! Whether each connected device is genuine Frostsnap hardware. See `docs/genuine-check-design.md`.
//!
//! Cosmetic: nothing waits on it and a failed proof is ignored.

use crate::firmware::FirmwareVersion;
use crate::frostsnap_persist::{Attestation, GenuineCerts};
use frostsnap_comms::genuine_certificate::{verify_attestation, verify_identity, CertificateBody};
use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, GenuineChallenge};
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use frostsnap_core::schnorr_fun::Signature;
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

#[derive(Debug, Clone)]
pub enum GenuineResponse {
    Attestation(Box<Attestation>),
    IdentityProof(Signature),
}

#[derive(Clone, Copy)]
enum Request {
    Attestation,
    Identity(GenuineChallenge),
}

struct Session {
    port: String,
    firmware: FirmwareVersion,
    proven: bool,
    outstanding: Option<Request>,
}

pub struct GenuineCheck {
    factory_key: Point<EvenY>,
    certs: Arc<Mutex<GenuineCerts>>,
    sessions: HashMap<DeviceId, Session>,
    changes: Vec<(DeviceId, GenuineStatus)>,
}

impl GenuineCheck {
    /// `certs` must have been loaded against `factory_key`. The caller keeps a handle to persist
    /// what this stages.
    pub fn new(factory_key: Point<EvenY>, certs: Arc<Mutex<GenuineCerts>>) -> Self {
        Self {
            factory_key,
            certs,
            sessions: Default::default(),
            changes: Default::default(),
        }
    }

    /// Call on every announce. A re-announce on the same port with the same firmware continues
    /// the session; anything else is a new connection that has to prove itself again.
    pub fn connected(
        &mut self,
        id: DeviceId,
        port: &str,
        firmware: FirmwareVersion,
    ) -> Option<CoordinatorSendMessage> {
        let is_new = !self
            .sessions
            .get(&id)
            .is_some_and(|session| session.port == port && session.firmware == firmware);
        if is_new {
            self.sessions.remove(&id);
        }
        let session = self.sessions.entry(id).or_insert_with(|| Session {
            port: port.to_string(),
            firmware,
            proven: false,
            outstanding: None,
        });
        let request = if session.firmware.features().genuine_check
            && !session.proven
            && session.outstanding.is_none()
        {
            let request = if self.certs.lock().unwrap().get(id).is_some() {
                Request::Identity(GenuineChallenge::random(&mut rand::thread_rng()))
            } else {
                Request::Attestation
            };
            session.outstanding = Some(request);
            Some(request)
        } else {
            None
        };
        if is_new {
            self.push_status(id);
        }
        request.map(|request| request_message(id, request))
    }

    pub fn disconnected(&mut self, id: DeviceId) {
        self.sessions.remove(&id);
    }

    /// `from` must be the id of the device the response came from, not one from the message body.
    pub fn recv(
        &mut self,
        from: DeviceId,
        response: GenuineResponse,
    ) -> Option<CoordinatorSendMessage> {
        let session = self.sessions.get_mut(&from)?;
        let outstanding = session.outstanding.take();
        match (outstanding, response) {
            (Some(Request::Attestation), GenuineResponse::Attestation(attestation)) => {
                match verify_attestation(
                    &attestation.certificate,
                    self.factory_key,
                    from,
                    &attestation.ds_signature,
                ) {
                    Ok(body) => {
                        self.certs.lock().unwrap().insert(from, *attestation, body);
                        let challenge = GenuineChallenge::random(&mut rand::thread_rng());
                        session.outstanding = Some(Request::Identity(challenge));
                        self.push_status(from);
                        Some(request_message(from, Request::Identity(challenge)))
                    }
                    Err(error) => {
                        ignore(from, error);
                        None
                    }
                }
            }
            (Some(Request::Identity(challenge)), GenuineResponse::IdentityProof(signature)) => {
                match verify_identity(from, challenge, &signature) {
                    Ok(()) => {
                        session.proven = true;
                        self.push_status(from);
                    }
                    Err(error) => ignore(from, error),
                }
                None
            }
            (outstanding, _) => {
                session.outstanding = outstanding;
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
                firmware_supports_check: session.firmware.features().genuine_check,
            },
            (None, _) => GenuineStatus::Unattested {
                firmware_supports_check: session.firmware.features().genuine_check,
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

fn request_message(id: DeviceId, request: Request) -> CoordinatorSendMessage {
    let body = match request {
        Request::Attestation => CoordinatorSendBody::RequestGenuineAttestation,
        Request::Identity(challenge) => {
            CoordinatorSendBody::GenuineIdentityChallenge(Box::new(challenge))
        }
    };
    CoordinatorSendMessage::to(id, body)
}

fn ignore(from: DeviceId, error: frostsnap_comms::genuine_certificate::GenuineError) {
    event!(
        Level::WARN,
        device = from.to_string(),
        error = error.to_string(),
        "ignoring a genuine check response that did not verify"
    );
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::persist::{Persist, TakeStaged};
    use frostsnap_comms::factory::DS_KEY_SIZE_BITS;
    use frostsnap_comms::genuine_certificate::{
        attestation_message, sign_certificate, sign_identity_challenge, CaseColor,
    };
    use frostsnap_comms::Sha256Digest;
    use frostsnap_core::schnorr_fun::{
        self,
        fun::{KeyPair, Scalar},
    };
    use frostsnap_core::sha2::{Digest, Sha256};
    use rand::{rngs::StdRng, SeedableRng};
    use rsa::{pkcs1::EncodeRsaPublicKey, Pkcs1v15Sign, RsaPrivateKey};
    use std::sync::LazyLock;

    pub(crate) static FACTORY: LazyLock<KeyPair<EvenY>> =
        LazyLock::new(|| KeyPair::new_xonly(Scalar::random(&mut StdRng::seed_from_u64(1))));
    static DS_KEY: LazyLock<RsaPrivateKey> = LazyLock::new(|| {
        RsaPrivateKey::new(&mut StdRng::seed_from_u64(2), DS_KEY_SIZE_BITS).unwrap()
    });

    pub(crate) fn supported() -> FirmwareVersion {
        FirmwareVersion::new(Sha256Digest([0xab; 32]))
    }

    pub(crate) struct TestDevice {
        keypair: KeyPair,
        pub(crate) id: DeviceId,
    }

    impl TestDevice {
        pub(crate) fn new(seed: u64) -> Self {
            let keypair = KeyPair::new(Scalar::random(&mut StdRng::seed_from_u64(seed)));
            Self {
                id: DeviceId::new(keypair.public_key()),
                keypair,
            }
        }

        fn attestation_for(&self, id: DeviceId) -> Attestation {
            let certificate = sign_certificate(
                schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                DS_KEY.to_public_key().to_pkcs1_der().unwrap().to_vec(),
                CaseColor::Orange,
                "2.7-1625".to_string(),
                "220825002".to_string(),
                1971,
                *FACTORY,
            );
            let digest: [u8; 32] = Sha256::digest(attestation_message(id)).into();
            let ds_signature = DS_KEY
                .sign(Pkcs1v15Sign::new::<Sha256>(), &digest)
                .unwrap()
                .try_into()
                .unwrap();
            Attestation {
                certificate,
                ds_signature: Box::new(ds_signature),
            }
        }

        pub(crate) fn answer(&self, request: &CoordinatorSendMessage) -> GenuineResponse {
            match &request.message_body {
                CoordinatorSendBody::RequestGenuineAttestation => {
                    GenuineResponse::Attestation(Box::new(self.attestation_for(self.id)))
                }
                CoordinatorSendBody::GenuineIdentityChallenge(challenge) => {
                    GenuineResponse::IdentityProof(sign_identity_challenge(
                        &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                        &self.keypair,
                        **challenge,
                    ))
                }
                other => panic!("not a genuine check request: {other:?}"),
            }
        }
    }

    fn new_check() -> GenuineCheck {
        GenuineCheck::new(FACTORY.public_key(), Default::default())
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
        port: &str,
    ) -> Vec<CoordinatorSendBody> {
        let mut sent = vec![];
        let mut request = check.connected(device.id, port, supported());
        while let Some(message) = request {
            request = check.recv(device.id, device.answer(&message));
            sent.push(message.message_body);
        }
        sent
    }

    fn has_staged_certs(check: &GenuineCheck) -> bool {
        check.certs.lock().unwrap().take_staged_update().is_some()
    }

    fn attested_check(device: &TestDevice) -> GenuineCheck {
        let mut check = new_check();
        prove(&mut check, device, "setup");
        check.disconnected(device.id);
        check.take_changes();
        check.certs.lock().unwrap().take_staged_update();
        check
    }

    #[test]
    fn a_new_port_is_a_new_connection_that_must_prove_itself() {
        let device = TestDevice::new(10);
        let mut check = new_check();
        prove(&mut check, &device, "a");
        assert!(is_genuine(&check, device.id));

        assert!(check.connected(device.id, "a", supported()).is_none());
        assert!(is_genuine(&check, device.id));

        check.take_changes();
        let request = check
            .connected(device.id, "b", supported())
            .expect("the new connection is challenged");
        assert!(matches!(
            check.take_changes().as_slice(),
            [(_, GenuineStatus::Attested { .. })]
        ));
        assert!(!is_genuine(&check, device.id));

        check.recv(device.id, device.answer(&request));
        assert!(is_genuine(&check, device.id));
    }

    #[test]
    fn a_malformed_stored_row_does_not_stop_the_load() -> anyhow::Result<()> {
        let device = TestDevice::new(11);
        let temp_file = tempfile::NamedTempFile::new()?;
        let mut conn = rusqlite::Connection::open(temp_file.path())?;
        GenuineCerts::migrate(&mut conn)?;

        let mut check = new_check();
        prove(&mut check, &device, "a");
        {
            let mut certs = check.certs.lock().unwrap();
            let update = certs.take_staged_update().unwrap();
            certs.persist_update(&mut conn, update)?;
        }
        conn.execute(
            "INSERT INTO fs_genuine_certs (id, certificate, ds_signature) VALUES ('not an id', x'00', x'00')",
            [],
        )?;

        let loaded = GenuineCerts::load(&mut conn, FACTORY.public_key())?;
        assert!(loaded.get(device.id).is_some());
        Ok(())
    }

    #[test]
    fn a_device_with_no_certificate_goes_1_2_3_in_two_queries_and_its_certificate_is_persisted(
    ) -> anyhow::Result<()> {
        let device = TestDevice::new(20);
        let mut check = new_check();

        let attest = check.connected(device.id, "a", supported()).unwrap();
        assert!(matches!(
            attest.message_body,
            CoordinatorSendBody::RequestGenuineAttestation
        ));
        assert_eq!(
            check.status(device.id),
            Some(GenuineStatus::Unattested {
                firmware_supports_check: true
            })
        );

        let challenge = check.recv(device.id, device.answer(&attest)).unwrap();
        assert!(matches!(
            challenge.message_body,
            CoordinatorSendBody::GenuineIdentityChallenge(_)
        ));
        assert!(matches!(
            check.status(device.id),
            Some(GenuineStatus::Attested { .. })
        ));

        assert!(check.recv(device.id, device.answer(&challenge)).is_none());
        assert!(is_genuine(&check, device.id));
        assert_eq!(check.take_changes().len(), 3);

        let temp_file = tempfile::NamedTempFile::new()?;
        let mut conn = rusqlite::Connection::open(temp_file.path())?;
        GenuineCerts::migrate(&mut conn)?;
        {
            let mut certs = check.certs.lock().unwrap();
            let update = certs
                .take_staged_update()
                .expect("the attestation is staged");
            certs.persist_update(&mut conn, update)?;
        }
        let reloaded = GenuineCerts::load(&mut conn, FACTORY.public_key())?;
        assert_eq!(
            reloaded.get(device.id).unwrap().case_color(),
            CaseColor::Orange
        );
        Ok(())
    }

    #[test]
    fn a_device_with_a_certificate_on_file_goes_2_3_in_one_query_with_no_ds_signature() {
        let device = TestDevice::new(21);
        let mut check = attested_check(&device);

        let sent = prove(&mut check, &device, "a");
        assert!(matches!(
            sent.as_slice(),
            [CoordinatorSendBody::GenuineIdentityChallenge(_)]
        ));
        assert!(is_genuine(&check, device.id));
        assert!(!has_staged_certs(&check));
    }

    #[test]
    fn a_failed_attestation_leaves_state_and_storage_unchanged() {
        let device = TestDevice::new(22);
        let mut check = new_check();
        let attest = check.connected(device.id, "a", supported()).unwrap();
        check.take_changes();

        let for_another_device = device.attestation_for(TestDevice::new(99).id);
        assert!(check
            .recv(
                device.id,
                GenuineResponse::Attestation(Box::new(for_another_device))
            )
            .is_none());
        assert_eq!(
            check.status(device.id),
            Some(GenuineStatus::Unattested {
                firmware_supports_check: true
            })
        );
        assert!(check.take_changes().is_empty());
        assert!(!has_staged_certs(&check));

        // The request was used up, so even a valid answer now is unsolicited.
        assert!(check.recv(device.id, device.answer(&attest)).is_none());
        assert!(check.certs.lock().unwrap().get(device.id).is_none());
    }

    #[test]
    fn a_certificate_whose_ds_signature_covers_another_device_id_is_rejected() {
        let device = TestDevice::new(23);
        let impostor = TestDevice::new(24);
        let mut check = new_check();
        check.connected(impostor.id, "a", supported()).unwrap();

        let genuine_devices_attestation = device.attestation_for(device.id);
        check.recv(
            impostor.id,
            GenuineResponse::Attestation(Box::new(genuine_devices_attestation)),
        );
        assert!(check.certs.lock().unwrap().get(impostor.id).is_none());
        assert!(matches!(
            check.status(impostor.id),
            Some(GenuineStatus::Unattested { .. })
        ));
    }

    #[test]
    fn a_failed_identity_proof_leaves_state_and_storage_unchanged() {
        let device = TestDevice::new(25);
        let impostor = TestDevice::new(26);
        let mut check = attested_check(&device);

        let challenge = check.connected(device.id, "a", supported()).unwrap();
        check.take_changes();
        check.recv(device.id, impostor.answer(&challenge));

        assert!(matches!(
            check.status(device.id),
            Some(GenuineStatus::Attested { .. })
        ));
        assert!(check.take_changes().is_empty());
        assert!(!has_staged_certs(&check));
        assert!(check.certs.lock().unwrap().get(device.id).is_some());
    }

    #[test]
    fn a_disconnect_returns_a_device_to_state_2() {
        let device = TestDevice::new(27);
        let mut check = new_check();
        prove(&mut check, &device, "a");
        assert!(is_genuine(&check, device.id));

        check.disconnected(device.id);
        assert_eq!(check.status(device.id), None);

        check.connected(device.id, "a", supported());
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

        let first = check.connected(device.id, "a", supported()).unwrap();
        let old_answer = device.answer(&first);
        check.disconnected(device.id);

        let second = check.connected(device.id, "a", supported()).unwrap();
        let (
            CoordinatorSendBody::GenuineIdentityChallenge(first),
            CoordinatorSendBody::GenuineIdentityChallenge(second),
        ) = (&first.message_body, &second.message_body)
        else {
            panic!("expected identity challenges");
        };
        assert_ne!(first, second);

        check.recv(device.id, old_answer);
        assert!(!is_genuine(&check, device.id));
    }

    #[test]
    fn firmware_without_the_feature_is_never_sent_either_request() {
        let unattested = TestDevice::new(29);
        let attested = TestDevice::new(30);
        let mut check = attested_check(&attested);

        for device in [&unattested, &attested] {
            assert!(check.connected(device.id, "a", unsupported()).is_none());
            assert!(check.connected(device.id, "a", unsupported()).is_none());
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
        check.connected(device.id, "a", unsupported());
        check.take_changes();

        check.recv(
            device.id,
            GenuineResponse::Attestation(Box::new(device.attestation_for(device.id))),
        );
        assert!(check.take_changes().is_empty());
        assert!(!has_staged_certs(&check));
    }
}
