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
            _ => "Black",
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

pub struct CertificateVerifier;

impl CertificateVerifier {
    /// Sign a new genuine certificate using the factory keypair
    pub fn sign<NG: NonceGen>(
        schnorr: Schnorr<Sha256, NG>,
        // RSA der bytes
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

    /// Verify a genuine certificate before accessing the contents
    pub fn verify(certificate: &Certificate, factory_key: Point<EvenY>) -> Option<CertificateBody> {
        match &certificate.factory_signature {
            frostsnap_core::Versioned::V0(factory_signature) => {
                if factory_key != factory_signature.factory_key {
                    // TODO: return error of UnknownFactoryKey
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

        let certificate = CertificateVerifier::sign(
            schnorr,
            ds_public_key.to_pkcs1_der().unwrap().to_vec(),
            CaseColor::Orange,
            "2.7-1625".to_string(), // BOARD_REVISION
            "220825002".to_string(),
            1971,
            factory_keypair,
        );

        let verified_cert =
            CertificateVerifier::verify(&certificate, factory_keypair.public_key()).unwrap();

        std::dbg!(verified_cert.serial_number());
    }
}
