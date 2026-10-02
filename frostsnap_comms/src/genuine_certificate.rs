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

    /// Unix seconds, UTC.
    pub fn provisioned_at(&self) -> u64 {
        match self {
            CertificateBody::Frontier { timestamp, .. } => *timestamp,
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
        bincode::encode_to_vec(&certificate_body, crate::SIGNING_BINCODE_CONFIG).unwrap();
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
                bincode::encode_to_vec(&certificate.body, crate::SIGNING_BINCODE_CONFIG).unwrap();
            let message = Message::new("frostsnap-genuine-key", &certificate_bytes);
            let schnorr = Schnorr::<Sha256>::verify_only();
            schnorr
                .verify(&factory_key, message, &factory_signature.signature)
                .then_some(certificate.body.clone())
        }
    }
}

/// A signature from a device's DS (RSA-3072) key.
#[derive(Clone, PartialEq, Eq)]
pub struct DsSignature(pub [u8; 384]);

frostsnap_core::impl_display_debug_serialize! {
    fn to_bytes(signature: &DsSignature) -> [u8;384] {
        signature.0
    }
}

frostsnap_core::impl_fromstr_deserialize! {
    name => "DS signature",
    fn from_bytes(bytes: [u8;384]) -> DsSignature {
        DsSignature(bytes)
    }
}

#[cfg(feature = "coordinator")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenuineError {
    DeviceIdMismatch,
    UnknownFactoryKey,
    CertificateSignatureInvalid,
    MalformedDsKey,
    FactoryAttestationSignatureInvalid,
    IdentityAttestationSignatureInvalid,
}

#[cfg(feature = "coordinator")]
impl core::fmt::Display for GenuineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            GenuineError::DeviceIdMismatch => "factory attestation is for another device",
            GenuineError::UnknownFactoryKey => "certificate signed by an unknown factory key",
            GenuineError::CertificateSignatureInvalid => "factory certificate signature invalid",
            GenuineError::MalformedDsKey => "malformed DS public key in certificate",
            GenuineError::FactoryAttestationSignatureInvalid => {
                "factory attestation signature invalid"
            }
            GenuineError::IdentityAttestationSignatureInvalid => {
                "identity attestation signature invalid"
            }
        };
        write!(f, "{s}")
    }
}

#[cfg(feature = "coordinator")]
impl core::error::Error for GenuineError {}

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
                bincode::encode_to_vec(&certificate.body, crate::SIGNING_BINCODE_CONFIG).unwrap();
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
}
