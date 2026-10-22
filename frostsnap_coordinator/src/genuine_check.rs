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
//! A DS key belongs to one piece of hardware, so once a device proves its identity, every other
//! device id's factory attestation naming the same DS key is deleted and that id's check is
//! dropped. A device reset or reseeded under a new id therefore does not leave its old id genuine.
//! A factory attestation alone supersedes nothing: until its device proves its identity, it sits
//! alongside the old id's, which stays genuine. Superseding needs the DS key to sign for the new
//! id, and anyone holding that key can already pass as that hardware. A superseded id that
//! connects again has no attestation on file, so it starts over, and if it proves itself it
//! supersedes in turn.
//!
//! Cosmetic: nothing waits on it and a failed attestation is ignored.

use crate::firmware::FirmwareVersion;
use crate::persist::{BincodeWrapper, Persist};
use bdk_chain::rusqlite_impl::migrate_schema;
use frostsnap_comms::genuine_certificate::{CertificateBody, GenuineError};
use frostsnap_comms::genuine_check::{
    self, AttestationSignature, AttestedDevice, AttestedDeviceDigest, RequestIdentityAttestation,
};
use frostsnap_comms::GenuineChallenge;
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use frostsnap_core::DeviceId;
use rand_core::RngCore;
use rusqlite::params;
use std::collections::{HashMap, VecDeque};
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

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct FactoryAttestation {
    pub attested: AttestedDevice,
    pub signature: Box<AttestationSignature>,
    /// The challenge this coordinator sent, which the signature covers.
    pub challenge: GenuineChallenge,
}

#[derive(Clone, Debug)]
pub enum GenuineCheckMutation {
    /// `body` is what `attestation` verified to.
    Insert {
        attestation: Box<FactoryAttestation>,
        body: Box<CertificateBody>,
    },
    Remove(DeviceId),
}

struct Session {
    firmware_supports_check: bool,
    proven: bool,
    outstanding: Option<genuine_check::CoordinatorMessage>,
}

pub struct GenuineCheck {
    factory_key: Point<EvenY>,
    /// Only attestations that verified against `factory_key` are kept, so holding one is holding
    /// evidence.
    certs: HashMap<DeviceId, (FactoryAttestation, CertificateBody)>,
    sessions: HashMap<DeviceId, Session>,
    events: Vec<(DeviceId, GenuineStatus)>,
}

impl GenuineCheck {
    /// A check with no factory attestations on file.
    pub fn new(factory_key: Point<EvenY>) -> Self {
        Self {
            factory_key,
            certs: Default::default(),
            sessions: Default::default(),
            events: Default::default(),
        }
    }

    /// Call on every announce. Each one is a new connection that has to prove itself again.
    pub fn connected(
        &mut self,
        id: DeviceId,
        firmware: FirmwareVersion,
        rng: &mut impl RngCore,
    ) -> Option<genuine_check::CoordinatorMessage> {
        let firmware_supports_check = firmware.features().genuine_check;
        let request = firmware_supports_check.then(|| {
            let challenge = GenuineChallenge::random(rng);
            match self.attested_digest(id) {
                Some(attested_digest) => {
                    genuine_check::CoordinatorMessage::RequestIdentityAttestation(
                        RequestIdentityAttestation {
                            challenge,
                            attested_digest,
                        },
                    )
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
        self.push_event(id);
        request
    }

    pub fn disconnected(&mut self, id: DeviceId) {
        self.sessions.remove(&id);
    }

    /// `from` must be the id of the device the message came from, not one from the message body.
    pub fn recv(
        &mut self,
        from: DeviceId,
        message: genuine_check::DeviceMessage,
        update: &mut VecDeque<GenuineCheckMutation>,
        rng: &mut impl RngCore,
    ) -> Option<RequestIdentityAttestation> {
        let session = self.sessions.get_mut(&from)?;
        match (&session.outstanding, message) {
            (
                Some(request),
                genuine_check::DeviceMessage::FactoryAttestation {
                    attested,
                    signature,
                },
            ) => {
                let challenge = request.challenge();
                match genuine_check::verify_factory_attestation(
                    &attested,
                    self.factory_key,
                    challenge,
                    from,
                    &signature,
                ) {
                    Ok(body) => {
                        let request = RequestIdentityAttestation {
                            challenge: GenuineChallenge::random(rng),
                            attested_digest: attested.digest(),
                        };
                        session.outstanding = Some(
                            genuine_check::CoordinatorMessage::RequestIdentityAttestation(
                                request.clone(),
                            ),
                        );
                        self.mutate(
                            GenuineCheckMutation::Insert {
                                attestation: Box::new(FactoryAttestation {
                                    attested: *attested,
                                    signature,
                                    challenge,
                                }),
                                body: Box::new(body),
                            },
                            update,
                        );
                        self.push_event(from);
                        Some(request)
                    }
                    Err(error) => {
                        ignore(from, error);
                        None
                    }
                }
            }
            (
                Some(genuine_check::CoordinatorMessage::RequestIdentityAttestation(
                    RequestIdentityAttestation {
                        challenge,
                        attested_digest,
                    },
                )),
                genuine_check::DeviceMessage::IdentityAttestation { signature },
            ) => {
                match genuine_check::verify_identity_attestation(
                    from,
                    *challenge,
                    *attested_digest,
                    &signature,
                ) {
                    Ok(()) => {
                        session.outstanding = None;
                        session.proven = true;
                        self.supersede(from, update);
                        self.push_event(from);
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

    /// `None` unless `id` is connected.
    pub fn status(&self, id: DeviceId) -> Option<GenuineStatus> {
        let session = self.sessions.get(&id)?;
        let certificate = self.certificate(id).cloned().map(Box::new);
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

    /// The certificate of the factory attestation on file for `id`, whether or not it is connected.
    pub fn certificate(&self, id: DeviceId) -> Option<&CertificateBody> {
        self.certs.get(&id).map(|(_, body)| body)
    }

    pub fn take_events(&mut self) -> Vec<(DeviceId, GenuineStatus)> {
        core::mem::take(&mut self.events)
    }

    fn attested_digest(&self, id: DeviceId) -> Option<AttestedDeviceDigest> {
        self.certs
            .get(&id)
            .map(|(attestation, _)| attestation.attested.digest())
    }

    fn apply_mutation(&mut self, mutation: &GenuineCheckMutation) -> bool {
        match mutation {
            GenuineCheckMutation::Insert { attestation, body } => {
                let entry = ((**attestation).clone(), (**body).clone());
                let id = attestation.attested.device_id();
                if self.certs.get(&id) == Some(&entry) {
                    return false;
                }
                self.certs.insert(id, entry);
                true
            }
            GenuineCheckMutation::Remove(id) => {
                if self.certs.remove(id).is_none() {
                    return false;
                }
                if let Some(session) = self.sessions.get_mut(id) {
                    session.proven = false;
                    session.outstanding = None;
                }
                self.push_event(*id);
                true
            }
        }
    }

    fn mutate(
        &mut self,
        mutation: GenuineCheckMutation,
        update: &mut VecDeque<GenuineCheckMutation>,
    ) {
        if self.apply_mutation(&mutation) {
            update.push_back(mutation);
        }
    }

    fn supersede(&mut self, id: DeviceId, update: &mut VecDeque<GenuineCheckMutation>) {
        let Some((_, body)) = self.certs.get(&id) else {
            return;
        };
        let ds_public_key = body.ds_public_key().clone();
        let superseded: Vec<DeviceId> = self
            .certs
            .iter()
            .filter(|(other, (_, body))| **other != id && *body.ds_public_key() == ds_public_key)
            .map(|(other, _)| *other)
            .collect();
        for old_id in superseded {
            event!(
                Level::INFO,
                device = id.to_string(),
                superseded = old_id.to_string(),
                "superseded another device id's factory attestation for the same DS key"
            );
            self.mutate(GenuineCheckMutation::Remove(old_id), update);
        }
    }

    fn push_event(&mut self, id: DeviceId) {
        if let Some(status) = self.status(id) {
            self.events.push((id, status));
        }
    }
}

fn ignore(from: DeviceId, error: GenuineError) {
    event!(
        Level::WARN,
        device = from.to_string(),
        error = error.to_string(),
        "ignoring a genuine check response that did not verify"
    );
}

impl Persist<rusqlite::Connection> for GenuineCheck {
    type Update = VecDeque<GenuineCheckMutation>;
    type LoadParams = Point<EvenY>;

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_genuine_attested_devices";
        const MIGRATIONS: &[&str] = &[
            // Version 0
            "CREATE TABLE IF NOT EXISTS fs_genuine_attested_devices ( \
                id BLOB PRIMARY KEY, \
                attestation BLOB NOT NULL \
            )",
        ];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    /// Rows are re-verified against `factory_key` and their row id. One that doesn't decode or
    /// verify (a database from a build with another factory key, or corruption) is skipped: that
    /// device attests again and its row is replaced.
    fn load(conn: &mut rusqlite::Connection, factory_key: Point<EvenY>) -> anyhow::Result<Self> {
        let mut stmt = conn.prepare("SELECT id, attestation FROM fs_genuine_attested_devices")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, DeviceId>(0).ok(),
                row.get::<_, BincodeWrapper<FactoryAttestation>>(1).ok(),
            ))
        })?;

        let mut check = GenuineCheck::new(factory_key);
        for row in rows {
            let (Some(device_id), Some(BincodeWrapper(attestation))) = row? else {
                event!(
                    Level::WARN,
                    "skipping a malformed stored genuine attestation"
                );
                continue;
            };
            match genuine_check::verify_factory_attestation(
                &attestation.attested,
                factory_key,
                attestation.challenge,
                device_id,
                &attestation.signature,
            ) {
                Ok(body) => {
                    check.apply_mutation(&GenuineCheckMutation::Insert {
                        attestation: Box::new(attestation),
                        body: Box::new(body),
                    });
                }
                Err(_) => event!(
                    Level::WARN,
                    device = device_id.to_string(),
                    "skipping a stored genuine attestation that no longer verifies"
                ),
            }
        }
        Ok(check)
    }

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
        for mutation in update {
            match mutation {
                GenuineCheckMutation::Insert { attestation, .. } => {
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_genuine_attested_devices (id, attestation) \
                         VALUES (?1, ?2)",
                        params![
                            attestation.attested.device_id(),
                            BincodeWrapper(*attestation)
                        ],
                    )?;
                }
                GenuineCheckMutation::Remove(device_id) => {
                    conn.execute(
                        "DELETE FROM fs_genuine_attested_devices WHERE id = ?1",
                        params![device_id],
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use frostsnap_comms::factory::DS_KEY_SIZE_BITS;
    use frostsnap_comms::genuine_certificate::{self, CaseColor, Certificate, DsSignature};
    use frostsnap_comms::genuine_check::{CoordinatorMessage, DsSignedMessage};
    use frostsnap_comms::Sha256Digest;
    use frostsnap_core::schnorr_fun::{
        self,
        fun::{KeyPair, Scalar},
    };
    use frostsnap_core::sha2::Sha256;
    use rand::{rngs::StdRng, SeedableRng};
    use rsa::{Pkcs1v15Sign, RsaPrivateKey};
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
            let certificate = genuine_certificate::sign_certificate(
                schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                CertificateBody::Frontier {
                    ds_public_key: (&ds_key.to_public_key()).into(),
                    case_color,
                    revision: "2.7-1625".to_string(),
                    serial: "220825002".to_string(),
                    timestamp: 1971,
                },
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
            let ds_signature = self
                .ds_key
                .sign(Pkcs1v15Sign::new::<Sha256>(), &signed.digest().0)
                .unwrap()
                .try_into()
                .unwrap();
            genuine_check::DeviceMessage::FactoryAttestation {
                attested: Box::new(attested),
                signature: Box::new(AttestationSignature::Rsa3072(DsSignature(ds_signature))),
            }
        }

        fn identity_attest(
            &self,
            challenge: GenuineChallenge,
            attested_digest: AttestedDeviceDigest,
        ) -> genuine_check::DeviceMessage {
            genuine_check::DeviceMessage::IdentityAttestation {
                signature: genuine_check::sign_identity_attestation(
                    &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                    &self.keypair,
                    challenge,
                    attested_digest,
                ),
            }
        }

        /// What honest firmware answers.
        fn answer(&self, request: &CoordinatorMessage) -> genuine_check::DeviceMessage {
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

    /// Connects `device` and answers until the check asks nothing more. Returns what it asked.
    fn prove(
        check: &mut GenuineCheck,
        device: &TestDevice,
        rng: &mut StdRng,
    ) -> Vec<CoordinatorMessage> {
        let mut sent = vec![];
        let mut next = check.connected(device.id, supported(), rng);
        while let Some(request) = next {
            next = check
                .recv(
                    device.id,
                    device.answer(&request),
                    &mut VecDeque::new(),
                    rng,
                )
                .map(CoordinatorMessage::RequestIdentityAttestation);
            sent.push(request);
            assert!(sent.len() <= 2, "the check keeps asking");
        }
        sent
    }

    fn is_genuine(check: &GenuineCheck, id: DeviceId) -> bool {
        matches!(check.status(id), Some(GenuineStatus::Genuine { .. }))
    }

    #[test]
    fn a_device_attests_once_and_then_proves_its_identity_on_every_announce() {
        let device = TestDevice::new(20);
        let mut check = GenuineCheck::new(FACTORY.public_key());
        let mut rng = StdRng::seed_from_u64(0);

        assert!(matches!(
            prove(&mut check, &device, &mut rng).as_slice(),
            [
                CoordinatorMessage::RequestFactoryAttestation { .. },
                CoordinatorMessage::RequestIdentityAttestation(_),
            ]
        ));
        assert!(is_genuine(&check, device.id));

        check.disconnected(device.id);
        assert!(matches!(
            prove(&mut check, &device, &mut rng).as_slice(),
            [CoordinatorMessage::RequestIdentityAttestation(request)]
                if request.attested_digest == device.attested().digest()
        ));
        assert!(is_genuine(&check, device.id));
    }

    #[test]
    fn a_relayed_identity_attestation_never_makes_another_devices_certificate_genuine() {
        let victim = TestDevice::new(36);
        let attacker = TestDevice::certified(37, &OTHER_DS_KEY, CaseColor::Red);
        let mut check = GenuineCheck::new(FACTORY.public_key());
        let mut rng = StdRng::seed_from_u64(0);
        let mut update = VecDeque::new();

        let attest = check.connected(victim.id, supported(), &mut rng).unwrap();
        let forged = AttestedDevice::V0 {
            device_id: victim.id,
            certificate: attacker.certificate.clone(),
        };
        let challenge_for_forged = check
            .recv(
                victim.id,
                attacker.factory_attest(attest.challenge(), forged.clone()),
                &mut update,
                &mut rng,
            )
            .expect("the attacker's DS key really did sign it");
        assert_eq!(challenge_for_forged.attested_digest, forged.digest());

        check.recv(
            victim.id,
            attacker.identity_attest(challenge_for_forged.challenge, forged.digest()),
            &mut update,
            &mut rng,
        );
        assert!(!is_genuine(&check, victim.id));

        let relayed = victim.answer(&CoordinatorMessage::RequestIdentityAttestation(
            challenge_for_forged,
        ));
        assert!(matches!(
            relayed,
            genuine_check::DeviceMessage::FactoryAttestation { .. }
        ));
        assert!(!is_genuine(&check, victim.id));
        let challenge_for_victim = check
            .recv(victim.id, relayed, &mut update, &mut rng)
            .unwrap();
        check.recv(
            victim.id,
            victim.answer(&CoordinatorMessage::RequestIdentityAttestation(
                challenge_for_victim.clone(),
            )),
            &mut update,
            &mut rng,
        );
        assert!(check
            .recv(
                victim.id,
                attacker.factory_attest(challenge_for_victim.challenge, forged),
                &mut update,
                &mut rng,
            )
            .is_none());
        assert!(matches!(
            check.status(victim.id),
            Some(GenuineStatus::Genuine { certificate })
                if certificate.case_color() == CaseColor::Orange
        ));
    }

    #[test]
    fn a_proven_new_id_with_the_same_ds_key_supersedes_the_old_one() {
        let old = TestDevice::new(40);
        let new = TestDevice::new(41);
        let mut check = GenuineCheck::new(FACTORY.public_key());
        let mut rng = StdRng::seed_from_u64(0);
        prove(&mut check, &old, &mut rng);
        assert!(is_genuine(&check, old.id));

        prove(&mut check, &new, &mut rng);
        assert!(is_genuine(&check, new.id));
        assert!(check.certificate(old.id).is_none());
        assert_eq!(
            check.status(old.id),
            Some(GenuineStatus::Unattested {
                firmware_supports_check: true
            })
        );
    }
}
