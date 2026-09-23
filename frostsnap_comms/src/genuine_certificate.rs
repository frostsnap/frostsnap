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

pub const GENUINE_IDENTITY_MESSAGE_TAG: &str = "frostsnap-genuine-identity";

/// Domain separation from anything else the DS key might ever sign.
pub const GENUINE_ATTESTATION_MESSAGE_TAG: &[u8; 27] = b"frostsnap-genuine-attest-v1";

/// What the DS key signs to vouch for `device_id`. There is no challenge in it so the coordinator
/// can keep the signature and re-check it on every launch.
pub fn attestation_message(device_id: frostsnap_core::DeviceId) -> [u8; 60] {
    let mut message = [0u8; 60];
    message[..27].copy_from_slice(GENUINE_ATTESTATION_MESSAGE_TAG);
    message[27..].copy_from_slice(device_id.as_bytes());
    message
}

pub fn sign_identity_challenge<NG: NonceGen>(
    schnorr: &Schnorr<Sha256, NG>,
    device_keypair: &KeyPair,
    challenge: crate::GenuineChallenge,
) -> Signature {
    let xonly_keypair: KeyPair<EvenY> = (*device_keypair).into();
    let message = Message::new(GENUINE_IDENTITY_MESSAGE_TAG, &challenge.0);
    schnorr.sign(&xonly_keypair, message)
}

#[cfg(feature = "coordinator")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenuineError {
    UnknownFactoryKey,
    CertificateSignatureInvalid,
    MalformedDsKey,
    AttestationSignatureInvalid,
    IdentitySignatureInvalid,
}

#[cfg(feature = "coordinator")]
impl core::fmt::Display for GenuineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            GenuineError::UnknownFactoryKey => "certificate signed by an unknown factory key",
            GenuineError::CertificateSignatureInvalid => "factory certificate signature invalid",
            GenuineError::MalformedDsKey => "malformed DS public key in certificate",
            GenuineError::AttestationSignatureInvalid => "DS attestation signature invalid",
            GenuineError::IdentitySignatureInvalid => "device identity signature invalid",
        };
        write!(f, "{s}")
    }
}

#[cfg(feature = "coordinator")]
impl core::error::Error for GenuineError {}

/// Verify that the factory certified the DS key and the DS key vouches for `device_id`.
///
/// `device_id` must be the id of the device we are talking to, never one taken from a message
/// body.
#[cfg(feature = "coordinator")]
pub fn verify_attestation(
    certificate: &Certificate,
    factory_key: Point<EvenY>,
    device_id: frostsnap_core::DeviceId,
    ds_signature: &[u8; 384],
) -> Result<CertificateBody, GenuineError> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use sha2::Digest;

    let body = verify_certificate_detailed(certificate, factory_key)?;
    let ds_public_key = rsa::RsaPublicKey::from_pkcs1_der(body.ds_public_key())
        .map_err(|_| GenuineError::MalformedDsKey)?;
    let padding = rsa::Pkcs1v15Sign::new::<sha2::Sha256>();
    let digest: [u8; 32] = sha2::Sha256::digest(attestation_message(device_id)).into();
    ds_public_key
        .verify(padding, &digest, ds_signature.as_ref())
        .map_err(|_| GenuineError::AttestationSignatureInvalid)?;
    Ok(body)
}

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

#[cfg(feature = "coordinator")]
pub fn verify_identity(
    device_id: frostsnap_core::DeviceId,
    challenge: crate::GenuineChallenge,
    signature: &Signature,
) -> Result<(), GenuineError> {
    // `DeviceId::pubkey()` maps invalid bytes to the generator, whose secret key is 1.
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

    fn ds_sign_attestation(
        ds_private: &RsaPrivateKey,
        device_id: frostsnap_core::DeviceId,
    ) -> [u8; 384] {
        use sha2::Digest;
        let digest: [u8; 32] = sha2::Sha256::digest(attestation_message(device_id)).into();
        ds_private
            .sign(rsa::Pkcs1v15Sign::new::<sha2::Sha256>(), &digest)
            .unwrap()
            .try_into()
            .unwrap()
    }

    #[test]
    pub fn attestation_is_bound_to_the_device_id() {
        let mut test_rng = ChaCha20Rng::from_seed([7u8; 32]);
        let factory_keypair = KeyPair::new_xonly(Scalar::random(&mut test_rng));
        let ds_private =
            RsaPrivateKey::new(&mut test_rng, crate::factory::DS_KEY_SIZE_BITS).unwrap();
        let certificate = sign_certificate(
            schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>(),
            ds_private.to_public_key().to_pkcs1_der().unwrap().to_vec(),
            CaseColor::Blue,
            "2.7-1625".to_string(),
            "220825002".to_string(),
            1971,
            factory_keypair,
        );
        let device_id =
            frostsnap_core::DeviceId::new(KeyPair::new(Scalar::random(&mut test_rng)).public_key());
        let other_id =
            frostsnap_core::DeviceId::new(KeyPair::new(Scalar::random(&mut test_rng)).public_key());
        let ds_signature = ds_sign_attestation(&ds_private, device_id);

        let body = verify_attestation(
            &certificate,
            factory_keypair.public_key(),
            device_id,
            &ds_signature,
        )
        .unwrap();
        assert_eq!(body.case_color(), CaseColor::Blue);

        assert_eq!(
            verify_attestation(
                &certificate,
                factory_keypair.public_key(),
                other_id,
                &ds_signature
            ),
            Err(GenuineError::AttestationSignatureInvalid),
        );

        let other_factory = KeyPair::new_xonly(Scalar::random(&mut test_rng));
        assert_eq!(
            verify_attestation(
                &certificate,
                other_factory.public_key(),
                device_id,
                &ds_signature
            ),
            Err(GenuineError::UnknownFactoryKey),
        );
    }

    #[test]
    pub fn identity_proof_is_bound_to_key_and_challenge() {
        let mut test_rng = ChaCha20Rng::from_seed([8u8; 32]);
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>();
        let device_keypair = KeyPair::new(Scalar::random(&mut test_rng));
        let device_id = frostsnap_core::DeviceId::new(device_keypair.public_key());
        let other_keypair = KeyPair::new(Scalar::random(&mut test_rng));
        let challenge = crate::GenuineChallenge([9u8; 32]);

        let signature = sign_identity_challenge(&schnorr, &device_keypair, challenge);
        assert_eq!(verify_identity(device_id, challenge, &signature), Ok(()));
        assert_eq!(
            verify_identity(device_id, crate::GenuineChallenge([1u8; 32]), &signature),
            Err(GenuineError::IdentitySignatureInvalid),
        );
        let forged = sign_identity_challenge(&schnorr, &other_keypair, challenge);
        assert_eq!(
            verify_identity(device_id, challenge, &forged),
            Err(GenuineError::IdentitySignatureInvalid),
        );
        assert_eq!(
            verify_identity(frostsnap_core::DeviceId([0u8; 33]), challenge, &signature),
            Err(GenuineError::IdentitySignatureInvalid),
        );
    }
}
