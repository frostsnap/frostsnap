use alloc::{string::String, vec::Vec};
use frostsnap_core::{
    schnorr_fun::{
        fun::{marker::EvenY, KeyPair, Point},
        nonce::NonceGen,
        Message, Schnorr, Signature,
    },
    sha2::Sha256,
    Versioned,
};

pub const CERTIFICATE_BINCODE_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Fixint,
    bincode::config::NoLimit,
> = bincode::config::standard().with_fixed_int_encoding();

#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub enum CertificateBody {
    Frontier {
        ds_public_key: Vec<u8>,
        case_color: CaseColor,
        revision: String,
        serial: String,
        timestamp: u64,
    },
}

impl CertificateBody {
    pub fn serial_number(&self) -> String {
        match &self {
            // TODO maybe put revision number
            CertificateBody::Frontier { serial, .. } => format!("FS-F-{}", serial),
        }
    }

    pub fn raw_serial(&self) -> String {
        match &self {
            CertificateBody::Frontier { serial, .. } => serial.clone(),
        }
    }

    pub fn ds_public_key(&self) -> &Vec<u8> {
        match &self {
            CertificateBody::Frontier { ds_public_key, .. } => ds_public_key,
        }
    }

    pub fn case_color(&self) -> CaseColor {
        match self {
            CertificateBody::Frontier { case_color, .. } => *case_color,
        }
    }

    pub fn revision(&self) -> &str {
        match self {
            CertificateBody::Frontier { revision, .. } => revision,
        }
    }
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub struct FrostsnapFactorySignature {
    pub factory_key: Point<EvenY>, // NOT for verification, just to know which factory
    pub signature: Signature,
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub struct Certificate {
    body: CertificateBody,
    factory_signature: Versioned<FrostsnapFactorySignature>,
}

impl Certificate {
    /// Should not be trusted, but useful in logging factory failures
    pub fn unverified_raw_serial(&self) -> String {
        self.body.raw_serial()
    }

    /// The case colour claimed by the certificate, without verifying it (colour
    /// is cosmetic identity, shown regardless of genuine status).
    pub fn unverified_case_color(&self) -> CaseColor {
        self.body.case_color()
    }
}

#[derive(bincode::Encode, bincode::Decode, Debug, Copy, Clone, PartialEq)]
pub enum CaseColor {
    Black,
    Orange,
    Silver,
    Blue,
    Red,
    Unused0,
    Unused1,
    Unused2,
    Unused3,
    Unused4,
    Unused5,
    Unused6,
    Unused8,
    Unused9,
}

impl core::fmt::Display for CaseColor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            CaseColor::Black => "Black",
            CaseColor::Orange => "Orange",
            CaseColor::Silver => "Silver",
            CaseColor::Blue => "Blue",
            CaseColor::Red => "Red",
            // Colours this build has no name for. Only reachable from a newer
            // device; never store this string (see `FromStr`, which rejects it).
            _ => "Unknown",
        };
        write!(f, "{}", s)
    }
}

impl core::str::FromStr for CaseColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "black" => Ok(CaseColor::Black),
            "orange" => Ok(CaseColor::Orange),
            "silver" => Ok(CaseColor::Silver),
            "blue" => Ok(CaseColor::Blue),
            "red" => Ok(CaseColor::Red),
            _ => Err(format!("Invalid color: {}", s)),
        }
    }
}

/// Sign a new genuine certificate using the factory keypair
pub fn sign_certificate<NG: NonceGen>(
    schnorr: Schnorr<Sha256, NG>,
    ds_public_key: Vec<u8>,
    case_color: CaseColor,
    revision: String,
    serial: String,
    timestamp: u64,
    factory_keypair: KeyPair<EvenY>,
) -> Certificate {
    let certificate_body = CertificateBody::Frontier {
        ds_public_key,
        case_color,
        timestamp,
        revision,
        serial,
    };

    let certificate_bytes =
        bincode::encode_to_vec(&certificate_body, CERTIFICATE_BINCODE_CONFIG).unwrap();
    let message = Message::new("frostsnap-genuine-key", &certificate_bytes);
    let factory_signature = FrostsnapFactorySignature {
        factory_key: factory_keypair.public_key(),
        signature: schnorr.sign(&factory_keypair, message),
    };

    Certificate {
        body: certificate_body,
        factory_signature: Versioned::V0(factory_signature),
    }
}

/// Verify a genuine certificate's Schnorr signature against a known factory key
pub fn verify_certificate(
    certificate: &Certificate,
    factory_key: Point<EvenY>,
) -> Option<CertificateBody> {
    match &certificate.factory_signature {
        frostsnap_core::Versioned::V0(factory_signature) => {
            if factory_key != factory_signature.factory_key {
                return None;
            }

            let certificate_bytes =
                bincode::encode_to_vec(&certificate.body, CERTIFICATE_BINCODE_CONFIG).unwrap();
            let message = Message::new("frostsnap-genuine-key", &certificate_bytes);
            let schnorr = Schnorr::<Sha256>::verify_only();
            schnorr
                .verify(&factory_key, message, &factory_signature.signature)
                .then_some(certificate.body.clone())
        }
    }
}

/// Tag for the device-identity (schnorr) proof-of-possession over the challenge.
pub const GENUINE_IDENTITY_MESSAGE_TAG: &str = "frostsnap-genuine-identity";

/// Domain-separation tag for the DS (RSA) genuine-proof message. The DS key signs
/// nothing else today; the tag guarantees any future use of the key can never
/// collide with a genuine proof.
pub const GENUINE_CHALLENGE_MESSAGE_TAG: &[u8; 20] = b"frostsnap-genuine-v1";

/// The message the device's DS (RSA) key signs: a fixed domain tag, then the
/// coordinator challenge, then the responder's own DeviceId. Binding the id is
/// what defeats relay/MITM: a genuine device only ever signs over its own id, so
/// its proof can't be replayed for another id.
pub fn genuine_challenge_message(
    challenge: crate::GenuineChallenge,
    device_id: frostsnap_core::DeviceId,
) -> [u8; 85] {
    let mut message = [0u8; 85];
    message[..20].copy_from_slice(GENUINE_CHALLENGE_MESSAGE_TAG);
    message[20..52].copy_from_slice(&challenge.0);
    message[52..].copy_from_slice(device_id.as_bytes());
    message
}

/// Sign the challenge with the device's identity (DeviceId) keypair, proving the
/// responder holds the DeviceId secret. Device-side counterpart to
/// [`verify_identity`].
pub fn sign_identity_challenge<NG: NonceGen>(
    schnorr: &Schnorr<Sha256, NG>,
    device_keypair: &KeyPair,
    challenge: crate::GenuineChallenge,
) -> Signature {
    let xonly_keypair: KeyPair<EvenY> = (*device_keypair).into();
    let message = Message::new(GENUINE_IDENTITY_MESSAGE_TAG, &challenge.0);
    schnorr.sign(&xonly_keypair, message)
}

/// Why a genuine check failed. Worth distinguishing: an unknown factory key is a
/// device we can't judge, quite different from a bad signature.
#[cfg(feature = "coordinator")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenuineError {
    /// The certificate was signed by a factory key we don't recognise.
    UnknownFactoryKey,
    /// The factory signature over the certificate body didn't verify.
    CertificateSignatureInvalid,
    /// The certificate's DS public key couldn't be parsed.
    MalformedDsKey,
    /// The RSA challenge-response (genuine-hardware proof, bound to the DeviceId)
    /// didn't verify.
    ChallengeSignatureInvalid,
    /// The schnorr identity proof (that the responder holds the DeviceId secret)
    /// didn't verify.
    IdentitySignatureInvalid,
}

#[cfg(feature = "coordinator")]
impl core::fmt::Display for GenuineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            GenuineError::UnknownFactoryKey => "certificate signed by an unknown factory key",
            GenuineError::CertificateSignatureInvalid => "factory certificate signature invalid",
            GenuineError::MalformedDsKey => "malformed DS public key in certificate",
            GenuineError::ChallengeSignatureInvalid => {
                "genuine-hardware challenge signature invalid"
            }
            GenuineError::IdentitySignatureInvalid => "device identity signature invalid",
        };
        write!(f, "{s}")
    }
}

#[cfg(feature = "coordinator")]
impl core::error::Error for GenuineError {}

/// Verify a full *bound* genuine proof: the factory-signed certificate, the DS
/// (RSA) signature over `tag ‖ challenge ‖ device_id`, and the DeviceId's own signature
/// over the challenge.
///
/// `device_id` must be the connection's `from` (the device we're actually talking
/// to), never a value from the message body.
#[cfg(feature = "coordinator")]
pub fn verify_genuine_bound(
    certificate: &Certificate,
    factory_key: Point<EvenY>,
    challenge: crate::GenuineChallenge,
    device_id: frostsnap_core::DeviceId,
    rsa_signature: &[u8; 384],
    identity_signature: &Signature,
) -> Result<CertificateBody, GenuineError> {
    let body = verify_certificate_detailed(certificate, factory_key)?;
    verify_challenge_bound(&body, challenge, device_id, rsa_signature)?;
    verify_identity(device_id, challenge, identity_signature)?;
    Ok(body)
}

/// Like [`verify_certificate`] but returns a typed [`GenuineError`] distinguishing
/// an unknown factory key from an invalid signature.
#[cfg(feature = "coordinator")]
pub fn verify_certificate_detailed(
    certificate: &Certificate,
    factory_key: Point<EvenY>,
) -> Result<CertificateBody, GenuineError> {
    match &certificate.factory_signature {
        frostsnap_core::Versioned::V0(factory_signature) => {
            if factory_key != factory_signature.factory_key {
                return Err(GenuineError::UnknownFactoryKey);
            }
            let certificate_bytes =
                bincode::encode_to_vec(&certificate.body, CERTIFICATE_BINCODE_CONFIG).unwrap();
            let message = Message::new("frostsnap-genuine-key", &certificate_bytes);
            let schnorr = Schnorr::<Sha256>::verify_only();
            if schnorr.verify(&factory_key, message, &factory_signature.signature) {
                Ok(certificate.body.clone())
            } else {
                Err(GenuineError::CertificateSignatureInvalid)
            }
        }
    }
}

/// Verify the RSA challenge-response, bound to `device_id`. The device signs
/// `SHA256(tag ‖ challenge ‖ device_id)` with its DS private key (see
/// [`genuine_challenge_message`]).
#[cfg(feature = "coordinator")]
pub fn verify_challenge_bound(
    certificate_body: &CertificateBody,
    challenge: crate::GenuineChallenge,
    device_id: frostsnap_core::DeviceId,
    signature: &[u8; 384],
) -> Result<(), GenuineError> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use sha2::Digest;

    let ds_public_key = rsa::RsaPublicKey::from_pkcs1_der(certificate_body.ds_public_key())
        .map_err(|_| GenuineError::MalformedDsKey)?;
    let padding = rsa::Pkcs1v15Sign::new::<sha2::Sha256>();
    let message = genuine_challenge_message(challenge, device_id);
    let message_digest: [u8; 32] = sha2::Sha256::digest(message).into();
    ds_public_key
        .verify(padding, &message_digest, signature.as_ref())
        .map_err(|_| GenuineError::ChallengeSignatureInvalid)
}

/// Verify the device-identity schnorr proof against `device_id`'s public key.
/// Counterpart to [`sign_identity_challenge`].
#[cfg(feature = "coordinator")]
pub fn verify_identity(
    device_id: frostsnap_core::DeviceId,
    challenge: crate::GenuineChallenge,
    signature: &Signature,
) -> Result<(), GenuineError> {
    // Reject a malformed DeviceId explicitly rather than relying on
    // `DeviceId::pubkey()`'s fallback, which returns a nullish point (the
    // generator, whose secret key is the known value 1) for invalid bytes.
    let point: Point =
        Point::from_bytes(*device_id.as_bytes()).ok_or(GenuineError::IdentitySignatureInvalid)?;
    let (xonly, _) = point.into_point_with_even_y();
    let message = Message::new(GENUINE_IDENTITY_MESSAGE_TAG, &challenge.0);
    let schnorr = Schnorr::<Sha256>::verify_only();
    if schnorr.verify(&xonly, message, signature) {
        Ok(())
    } else {
        Err(GenuineError::IdentitySignatureInvalid)
    }
}

#[cfg(test)]
mod test {
    use std::string::ToString;

    use super::*;
    use frostsnap_core::schnorr_fun::fun::{KeyPair, Scalar};
    use frostsnap_core::{schnorr_fun, sha2};
    use rand_chacha::rand_core::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    use rsa::pkcs1::EncodeRsaPublicKey;
    use rsa::RsaPrivateKey;

    #[test]
    pub fn certificate_sign_then_verify() {
        let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

        let factory_secret = Scalar::random(&mut test_rng);
        let factory_keypair = KeyPair::new_xonly(factory_secret);

        let ds_public_key = RsaPrivateKey::new(&mut test_rng, crate::factory::DS_KEY_SIZE_BITS)
            .unwrap()
            .to_public_key();

        let schnorr = schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>();

        let certificate = sign_certificate(
            schnorr,
            ds_public_key.to_pkcs1_der().unwrap().to_vec(),
            CaseColor::Orange,
            "2.7-1625".to_string(), // BOARD_REVISION
            "220825002".to_string(),
            1971,
            factory_keypair,
        );

        let verified_cert = verify_certificate(&certificate, factory_keypair.public_key()).unwrap();

        std::dbg!(verified_cert.serial_number());
    }

    /// Simulate the device side of a bound genuine proof: RSA-sign
    /// `SHA256(tag ‖ challenge ‖ device_id)` with the DS key and schnorr-sign the
    /// challenge with the device identity key.
    fn make_bound_proof(
        ds_private: &RsaPrivateKey,
        device_keypair: &KeyPair,
        challenge: crate::GenuineChallenge,
    ) -> (alloc::boxed::Box<[u8; 384]>, Signature) {
        use rsa::Pkcs1v15Sign;
        use sha2::Digest;

        let device_id = frostsnap_core::DeviceId::new(device_keypair.public_key());
        let message = genuine_challenge_message(challenge, device_id);
        let digest: [u8; 32] = sha2::Sha256::digest(message).into();
        let sig_vec = ds_private
            .sign(Pkcs1v15Sign::new::<sha2::Sha256>(), &digest)
            .unwrap();
        let rsa_signature: alloc::boxed::Box<[u8; 384]> =
            alloc::boxed::Box::new(sig_vec.try_into().expect("3072-bit key => 384-byte sig"));

        let schnorr = schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>();
        let identity_signature = sign_identity_challenge(&schnorr, device_keypair, challenge);

        (rsa_signature, identity_signature)
    }

    #[test]
    pub fn bound_genuine_proof_roundtrip_and_relay_resistance() {
        let mut test_rng = ChaCha20Rng::from_seed([7u8; 32]);

        let factory_keypair = KeyPair::new_xonly(Scalar::random(&mut test_rng));
        let ds_private =
            RsaPrivateKey::new(&mut test_rng, crate::factory::DS_KEY_SIZE_BITS).unwrap();

        let schnorr = schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>();
        let certificate = sign_certificate(
            schnorr,
            ds_private.to_public_key().to_pkcs1_der().unwrap().to_vec(),
            CaseColor::Blue,
            "2.7-1625".to_string(),
            "220825002".to_string(),
            1971,
            factory_keypair,
        );

        let device_keypair = KeyPair::new(Scalar::random(&mut test_rng));
        let device_id = frostsnap_core::DeviceId::new(device_keypair.public_key());

        let challenge = crate::GenuineChallenge([9u8; 32]);
        let (rsa_signature, identity_signature) =
            make_bound_proof(&ds_private, &device_keypair, challenge);

        // Happy path: the coordinator verifies against the id it is talking to.
        let body = verify_genuine_bound(
            &certificate,
            factory_keypair.public_key(),
            challenge,
            device_id,
            &rsa_signature,
            &identity_signature,
        )
        .expect("valid bound proof must verify");
        assert_eq!(body.case_color(), CaseColor::Blue);

        // Relay/MITM: a malicious device forwards this genuine device's proof but
        // claims a *different* id. The RSA signature is over the genuine id, so it
        // must fail when checked against the attacker's id.
        let attacker_keypair = KeyPair::new(Scalar::random(&mut test_rng));
        let attacker_id = frostsnap_core::DeviceId::new(attacker_keypair.public_key());
        assert_eq!(
            verify_genuine_bound(
                &certificate,
                factory_keypair.public_key(),
                challenge,
                attacker_id,
                &rsa_signature,
                &identity_signature,
            ),
            Err(GenuineError::ChallengeSignatureInvalid),
        );

        // Wrong challenge (stale/replayed) fails.
        assert_eq!(
            verify_genuine_bound(
                &certificate,
                factory_keypair.public_key(),
                crate::GenuineChallenge([1u8; 32]),
                device_id,
                &rsa_signature,
                &identity_signature,
            ),
            Err(GenuineError::ChallengeSignatureInvalid),
        );

        // Genuine RSA proof but a forged identity proof (a relay holder lacks the
        // DeviceId secret): the identity signature fails against the claimed id.
        let bad_identity = {
            let s = schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>();
            sign_identity_challenge(&s, &attacker_keypair, challenge)
        };
        assert_eq!(
            verify_genuine_bound(
                &certificate,
                factory_keypair.public_key(),
                challenge,
                device_id,
                &rsa_signature,
                &bad_identity,
            ),
            Err(GenuineError::IdentitySignatureInvalid),
        );

        // Unknown factory key is distinguished from a bad signature.
        let other_factory = KeyPair::new_xonly(Scalar::random(&mut test_rng));
        assert_eq!(
            verify_genuine_bound(
                &certificate,
                other_factory.public_key(),
                challenge,
                device_id,
                &rsa_signature,
                &identity_signature,
            ),
            Err(GenuineError::UnknownFactoryKey),
        );

        // A malformed DeviceId is rejected outright, not silently treated as a
        // nullish key.
        assert_eq!(
            verify_identity(
                frostsnap_core::DeviceId([0u8; 33]),
                challenge,
                &identity_signature,
            ),
            Err(GenuineError::IdentitySignatureInvalid),
        );
    }
}
