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
        ds_public_key: DsPublicKeyDer,
        case_color: CaseColor,
        revision: String,
        serial: String,
        timestamp: u64,
    },
}

/// PKCS#1 DER encoding of a device's RSA-3072 DS public key.
#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq, Eq)]
pub struct DsPublicKeyDer(pub Vec<u8>);

impl DsPublicKeyDer {
    #[cfg(feature = "coordinator")]
    pub fn to_rsa(&self) -> Result<rsa::RsaPublicKey, GenuineError> {
        use rsa::pkcs1::DecodeRsaPublicKey;
        rsa::RsaPublicKey::from_pkcs1_der(&self.0).map_err(|_| GenuineError::MalformedDsKey)
    }
}

#[cfg(feature = "rsa")]
impl From<&rsa::RsaPublicKey> for DsPublicKeyDer {
    fn from(key: &rsa::RsaPublicKey) -> Self {
        use rsa::pkcs1::EncodeRsaPublicKey;
        DsPublicKeyDer(
            key.to_pkcs1_der()
                .expect("an RSA public key always has a DER encoding")
                .to_vec(),
        )
    }
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

    pub fn ds_public_key(&self) -> &DsPublicKeyDer {
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CaseColor {
    Black,
    Orange,
    Silver,
    Blue,
    Red,
    /// A colour this build has no name for.
    Other(u32),
}

impl CaseColor {
    fn index(self) -> u32 {
        match self {
            CaseColor::Black => 0,
            CaseColor::Orange => 1,
            CaseColor::Silver => 2,
            CaseColor::Blue => 3,
            CaseColor::Red => 4,
            CaseColor::Other(index) => index,
        }
    }

    fn from_index(index: u32) -> Self {
        match index {
            0 => CaseColor::Black,
            1 => CaseColor::Orange,
            2 => CaseColor::Silver,
            3 => CaseColor::Blue,
            4 => CaseColor::Red,
            index => CaseColor::Other(index),
        }
    }
}

// Encoded as the bare `u32` variant index, exactly as the derived unit-variant enum this replaced,
// so issued certificates still verify.
impl bincode::Encode for CaseColor {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.index().encode(encoder)
    }
}

impl<Context> bincode::Decode<Context> for CaseColor {
    fn decode<D: bincode::de::Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self::from_index(u32::decode(decoder)?))
    }
}

impl<'de, Context> bincode::BorrowDecode<'de, Context> for CaseColor {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(Self::from_index(u32::borrow_decode(decoder)?))
    }
}

impl core::fmt::Display for CaseColor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            CaseColor::Black => "Black",
            CaseColor::Orange => "Orange",
            CaseColor::Silver => "Silver",
            CaseColor::Blue => "Blue",
            CaseColor::Red => "Red",
            CaseColor::Other(_) => "Unknown",
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
    certificate_body: CertificateBody,
    factory_keypair: KeyPair<EvenY>,
) -> Certificate {
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
    use frostsnap_core::{hex, schnorr_fun, sha2};
    use rand_chacha::rand_core::SeedableRng;
    use rand_chacha::ChaCha20Rng;
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
            CertificateBody::Frontier {
                ds_public_key: DsPublicKeyDer::from(&ds_public_key),
                case_color: CaseColor::Orange,
                revision: "2.7-1625".to_string(), // BOARD_REVISION
                serial: "220825002".to_string(),
                timestamp: 1971,
            },
            factory_keypair,
        );

        let verified_cert = verify_certificate(&certificate, factory_keypair.public_key()).unwrap();

        std::dbg!(verified_cert.serial_number());

        let mut tampered = certificate;
        let CertificateBody::Frontier { ds_public_key, .. } = &mut tampered.body;
        *ds_public_key = DsPublicKeyDer(vec![9; 3]);
        assert_eq!(
            verify_certificate_detailed(&tampered, factory_keypair.public_key()),
            Err(GenuineError::CertificateSignatureInvalid)
        );
    }

    /// Bytes captured by encoding the derived unit-variant `CaseColor` at 55f346cd, the last
    /// revision before it gained `Other`. Old firmware and issued certificates contain exactly
    /// these, and nothing in the current code can regenerate them.
    /// `(colour, SIGNING_BINCODE_CONFIG bytes, BINCODE_CONFIG bytes)`.
    const HISTORICAL_CASE_COLORS: [(CaseColor, &str, &str); 14] = [
        (CaseColor::Black, "00000000", "00"),
        (CaseColor::Orange, "01000000", "01"),
        (CaseColor::Silver, "02000000", "02"),
        (CaseColor::Blue, "03000000", "03"),
        (CaseColor::Red, "04000000", "04"),
        (CaseColor::Other(5), "05000000", "05"),
        (CaseColor::Other(6), "06000000", "06"),
        (CaseColor::Other(7), "07000000", "07"),
        (CaseColor::Other(8), "08000000", "08"),
        (CaseColor::Other(9), "09000000", "09"),
        (CaseColor::Other(10), "0a000000", "0a"),
        (CaseColor::Other(11), "0b000000", "0b"),
        (CaseColor::Other(12), "0c000000", "0c"),
        (CaseColor::Other(13), "0d000000", "0d"),
    ];

    fn assert_frozen<C: bincode::config::Config>(expected: CaseColor, frozen_hex: &str, config: C) {
        let frozen = hex::decode(frozen_hex).unwrap();
        let decoded: (CaseColor, usize) = bincode::decode_from_slice(&frozen, config).unwrap();
        assert_eq!(decoded, (expected, frozen.len()), "{frozen_hex}");
        assert_eq!(
            bincode::encode_to_vec(expected, config).unwrap(),
            frozen,
            "{expected:?}"
        );
    }

    fn both_configs<T: bincode::Encode>(value: T) -> [Vec<u8>; 2] {
        [
            bincode::encode_to_vec(&value, crate::SIGNING_BINCODE_CONFIG).unwrap(),
            bincode::encode_to_vec(&value, crate::BINCODE_CONFIG).unwrap(),
        ]
    }

    fn decode_both(encodings: [Vec<u8>; 2]) -> [CaseColor; 2] {
        let [fixint, varint] = encodings;
        [
            bincode::decode_from_slice(&fixint, crate::SIGNING_BINCODE_CONFIG)
                .unwrap()
                .0,
            bincode::decode_from_slice(&varint, crate::BINCODE_CONFIG)
                .unwrap()
                .0,
        ]
    }

    #[test]
    fn case_color_decodes_and_reencodes_its_historical_bytes() {
        for (color, signing, wire) in HISTORICAL_CASE_COLORS {
            assert_frozen(color, signing, crate::SIGNING_BINCODE_CONFIG);
            assert_frozen(color, wire, crate::BINCODE_CONFIG);
        }
    }

    #[test]
    fn unknown_case_color_round_trips() {
        for index in [14u32, 99, 300] {
            let encodings = both_configs(index);
            assert_eq!(decode_both(encodings.clone()), [CaseColor::Other(index); 2]);
            assert_eq!(both_configs(CaseColor::Other(index)), encodings);
        }
    }

    #[test]
    fn ds_public_key_der_encodes_as_bare_bytes() {
        let der = vec![0xd5u8; 300];
        assert_eq!(both_configs(DsPublicKeyDer(der.clone())), both_configs(der));
    }
}
