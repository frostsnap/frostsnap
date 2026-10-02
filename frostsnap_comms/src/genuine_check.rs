//! The messages of the genuine check and the bytes each key signs.
//!
//! Everything signed is encoded with [`SIGNING_BINCODE_CONFIG`].

use crate::genuine_certificate::{Certificate, DsSignature};
use crate::{GenuineChallenge, SIGNING_BINCODE_CONFIG};
use alloc::boxed::Box;
use alloc::vec::Vec;
use frostsnap_core::schnorr_fun::{
    fun::{marker::EvenY, KeyPair},
    nonce::NonceGen,
    Message, Schnorr, Signature,
};
use frostsnap_core::sha2::{Digest, Sha256};
use frostsnap_core::DeviceId;

#[cfg(feature = "coordinator")]
use crate::genuine_certificate::{verify_certificate_detailed, CertificateBody, GenuineError};
#[cfg(feature = "coordinator")]
use frostsnap_core::schnorr_fun::fun::Point;

const IDENTITY_MESSAGE_TAG: &str = "frostsnap-genuine-identity";

/// What a device's factory attestation vouches for.
#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub enum AttestedDevice {
    V0 {
        device_id: DeviceId,
        certificate: Certificate,
    },
}

impl AttestedDevice {
    pub fn device_id(&self) -> DeviceId {
        match self {
            AttestedDevice::V0 { device_id, .. } => *device_id,
        }
    }

    pub fn certificate(&self) -> &Certificate {
        match self {
            AttestedDevice::V0 { certificate, .. } => certificate,
        }
    }

    pub fn to_signing_bytes(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, SIGNING_BINCODE_CONFIG).expect("infallible")
    }

    pub fn digest(&self) -> AttestedDeviceDigest {
        AttestedDeviceDigest(Sha256::digest(self.to_signing_bytes()).into())
    }
}

/// SHA256 of an [`AttestedDevice`]'s encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttestedDeviceDigest(pub [u8; 32]);

frostsnap_core::impl_display_debug_serialize! {
    fn to_bytes(digest: &AttestedDeviceDigest) -> [u8;32] {
        digest.0
    }
}

frostsnap_core::impl_fromstr_deserialize! {
    name => "attested device digest",
    fn from_bytes(bytes: [u8;32]) -> AttestedDeviceDigest {
        AttestedDeviceDigest(bytes)
    }
}

/// Everything the DS key signs, in a factory attestation. The DS key signs SHA256 of its
/// encoding.
#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub enum DsSignedMessage {
    GenuineAttestationV0 {
        challenge: GenuineChallenge,
        attested: AttestedDevice,
    },
}

impl DsSignedMessage {
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, SIGNING_BINCODE_CONFIG).expect("infallible")
    }
}

/// Everything the device-id key signs, in an identity attestation.
#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub enum IdentityMessage {
    V0 {
        challenge: GenuineChallenge,
        attested_digest: AttestedDeviceDigest,
    },
}

impl IdentityMessage {
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, SIGNING_BINCODE_CONFIG).expect("infallible")
    }
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub enum CoordinatorMessage {
    RequestFactoryAttestation {
        challenge: GenuineChallenge,
    },
    RequestIdentityAttestation {
        challenge: GenuineChallenge,
        attested_digest: AttestedDeviceDigest,
    },
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum DeviceMessage {
    /// `ds_signature` is over [`DsSignedMessage::GenuineAttestationV0`].
    FactoryAttestation {
        attested: Box<AttestedDevice>,
        ds_signature: Box<DsSignature>,
    },
    /// From [`sign_identity_attestation`].
    IdentityAttestation { signature: Signature },
}

/// How a device answers a [`CoordinatorMessage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAction {
    /// Send a [`DeviceMessage::FactoryAttestation`] of its own [`AttestedDevice`] under this
    /// challenge.
    FactoryAttest(GenuineChallenge),
    /// Send a [`DeviceMessage::IdentityAttestation`] over this challenge and its own digest.
    IdentityAttest(GenuineChallenge),
}

impl CoordinatorMessage {
    /// A device's id key only ever signs over `own`, the digest of its own [`AttestedDevice`]. Any
    /// other digest is answered by attesting again under the same challenge, so the coordinator
    /// replaces its record with the device's own.
    pub fn device_action(&self, own: AttestedDeviceDigest) -> DeviceAction {
        match self {
            CoordinatorMessage::RequestFactoryAttestation { challenge } => {
                DeviceAction::FactoryAttest(*challenge)
            }
            CoordinatorMessage::RequestIdentityAttestation {
                challenge,
                attested_digest,
            } if *attested_digest == own => DeviceAction::IdentityAttest(*challenge),
            CoordinatorMessage::RequestIdentityAttestation { challenge, .. } => {
                DeviceAction::FactoryAttest(*challenge)
            }
        }
    }

    pub fn challenge(&self) -> GenuineChallenge {
        match self {
            CoordinatorMessage::RequestFactoryAttestation { challenge }
            | CoordinatorMessage::RequestIdentityAttestation { challenge, .. } => *challenge,
        }
    }
}

/// Only ever call with the digest of the signing device's own [`AttestedDevice`].
pub fn sign_identity_attestation<NG: NonceGen>(
    schnorr: &Schnorr<Sha256, NG>,
    device_keypair: &KeyPair,
    challenge: GenuineChallenge,
    attested_digest: AttestedDeviceDigest,
) -> Signature {
    let xonly_keypair: KeyPair<EvenY> = (*device_keypair).into();
    let bytes = IdentityMessage::V0 {
        challenge,
        attested_digest,
    }
    .to_signing_bytes();
    schnorr.sign(&xonly_keypair, Message::new(IDENTITY_MESSAGE_TAG, &bytes))
}

#[cfg(feature = "coordinator")]
pub fn verify_identity_attestation(
    device_id: DeviceId,
    challenge: GenuineChallenge,
    attested_digest: AttestedDeviceDigest,
    signature: &Signature,
) -> Result<(), GenuineError> {
    // `DeviceId::pubkey()` maps invalid bytes to the generator, whose secret key is 1.
    let point: Point = Point::from_bytes(*device_id.as_bytes())
        .ok_or(GenuineError::IdentityAttestationSignatureInvalid)?;
    let (xonly, _) = point.into_point_with_even_y();
    let bytes = IdentityMessage::V0 {
        challenge,
        attested_digest,
    }
    .to_signing_bytes();
    let schnorr = Schnorr::<Sha256>::verify_only();
    if schnorr.verify(
        &xonly,
        Message::new(IDENTITY_MESSAGE_TAG, &bytes),
        signature,
    ) {
        Ok(())
    } else {
        Err(GenuineError::IdentityAttestationSignatureInvalid)
    }
}

/// Verify that `attested` is about `from`, that the factory certified its DS key, and that the DS
/// key signed it under `challenge`.
///
/// `from` must be the id of the device we are talking to, never one taken from a message body.
#[cfg(feature = "coordinator")]
pub fn verify_factory_attestation(
    attested: &AttestedDevice,
    factory_key: Point<EvenY>,
    challenge: GenuineChallenge,
    from: DeviceId,
    ds_signature: &DsSignature,
) -> Result<CertificateBody, GenuineError> {
    use rsa::pkcs1::DecodeRsaPublicKey;

    if attested.device_id() != from {
        return Err(GenuineError::DeviceIdMismatch);
    }
    let body = verify_certificate_detailed(attested.certificate(), factory_key)?;
    let ds_public_key = rsa::RsaPublicKey::from_pkcs1_der(body.ds_public_key())
        .map_err(|_| GenuineError::MalformedDsKey)?;
    let signed = DsSignedMessage::GenuineAttestationV0 {
        challenge,
        attested: attested.clone(),
    };
    let digest: [u8; 32] = Sha256::digest(signed.to_signing_bytes()).into();
    ds_public_key
        .verify(
            rsa::Pkcs1v15Sign::new::<sha2::Sha256>(),
            &digest,
            ds_signature.0.as_ref(),
        )
        .map_err(|_| GenuineError::FactoryAttestationSignatureInvalid)?;
    Ok(body)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::genuine_certificate::{sign_certificate, CaseColor};
    use frostsnap_core::hex;
    use frostsnap_core::schnorr_fun::{self, fun::Scalar};
    use rand_chacha::rand_core::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    use rsa::pkcs1::EncodeRsaPublicKey;
    use rsa::RsaPrivateKey;
    use std::string::ToString;

    fn keypair(byte: u8) -> KeyPair {
        KeyPair::new(Scalar::from_bytes_mod_order([byte; 32]).non_zero().unwrap())
    }

    fn certificate(factory: KeyPair<EvenY>, ds_public_key: Vec<u8>) -> Certificate {
        sign_certificate(
            schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
            ds_public_key,
            CaseColor::Orange,
            "2.7-1625".to_string(),
            "220825002".to_string(),
            1971,
            factory,
        )
    }

    fn ds_sign(ds_private: &RsaPrivateKey, bytes: &[u8]) -> DsSignature {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        DsSignature(
            ds_private
                .sign(rsa::Pkcs1v15Sign::new::<sha2::Sha256>(), &digest)
                .unwrap()
                .try_into()
                .unwrap(),
        )
    }

    #[test]
    fn attested_device_has_a_fixed_width_encoding_and_digest() {
        let factory = KeyPair::new_xonly(Scalar::from_bytes_mod_order([1; 32]).non_zero().unwrap());
        let device_id = DeviceId::new(keypair(2).public_key());
        let attested = AttestedDevice::V0 {
            device_id,
            certificate: certificate(factory, vec![0xd5; 3]),
        };

        let challenge = GenuineChallenge([0xcc; 32]);

        assert_eq!(hex::encode(&attested.to_signing_bytes()), "00000000024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766000000000300000000000000d5d5d5010000000800000000000000322e372d313632350900000000000000323230383235303032b307000000000000000000001b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f1ecdcc05fbbe5f41e0a421cffa06a2744adf5b1f607feabf3e5ec7cbfe4b57d9979a2eaacd8181f458c11a3ff188adc1829dacd6c3825fc1822aebb868fc081b");
        assert_eq!(
            hex::encode(
                &DsSignedMessage::GenuineAttestationV0 {
                    challenge,
                    attested: attested.clone(),
                }
                .to_signing_bytes()
            ),
            "00000000cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc00000000024d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766000000000300000000000000d5d5d5010000000800000000000000322e372d313632350900000000000000323230383235303032b307000000000000000000001b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f1ecdcc05fbbe5f41e0a421cffa06a2744adf5b1f607feabf3e5ec7cbfe4b57d9979a2eaacd8181f458c11a3ff188adc1829dacd6c3825fc1822aebb868fc081b"
        );
        assert_eq!(
            hex::encode(
                &IdentityMessage::V0 {
                    challenge,
                    attested_digest: attested.digest(),
                }
                .to_signing_bytes()
            ),
            "00000000cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc25317233f622d878844e0b3a6f4ea0e839445c7a043df1315872eef13faf948c"
        );
        assert_eq!(
            attested.digest().to_string(),
            "25317233f622d878844e0b3a6f4ea0e839445c7a043df1315872eef13faf948c"
        );
    }

    #[test]
    fn a_device_identity_attests_only_over_its_own_digest() {
        let own = AttestedDeviceDigest([1; 32]);
        let challenge = GenuineChallenge([2; 32]);

        assert_eq!(
            CoordinatorMessage::RequestIdentityAttestation {
                challenge,
                attested_digest: own
            }
            .device_action(own),
            DeviceAction::IdentityAttest(challenge)
        );
        assert_eq!(
            CoordinatorMessage::RequestIdentityAttestation {
                challenge,
                attested_digest: AttestedDeviceDigest([3; 32])
            }
            .device_action(own),
            DeviceAction::FactoryAttest(challenge)
        );
        assert_eq!(
            CoordinatorMessage::RequestFactoryAttestation { challenge }.device_action(own),
            DeviceAction::FactoryAttest(challenge)
        );
    }

    #[test]
    fn factory_attestation_is_bound_to_the_device_id_certificate_and_challenge() {
        let mut test_rng = ChaCha20Rng::from_seed([7u8; 32]);
        let factory = KeyPair::new_xonly(Scalar::random(&mut test_rng));
        let ds_private =
            RsaPrivateKey::new(&mut test_rng, crate::factory::DS_KEY_SIZE_BITS).unwrap();
        let device_id = DeviceId::new(keypair(3).public_key());
        let other_id = DeviceId::new(keypair(4).public_key());
        let attested = AttestedDevice::V0 {
            device_id,
            certificate: certificate(
                factory,
                ds_private.to_public_key().to_pkcs1_der().unwrap().to_vec(),
            ),
        };
        let challenge = GenuineChallenge([3u8; 32]);
        let ds_signature = ds_sign(
            &ds_private,
            &DsSignedMessage::GenuineAttestationV0 {
                challenge,
                attested: attested.clone(),
            }
            .to_signing_bytes(),
        );

        let body = verify_factory_attestation(
            &attested,
            factory.public_key(),
            challenge,
            device_id,
            &ds_signature,
        )
        .unwrap();
        assert_eq!(body.case_color(), CaseColor::Orange);

        assert_eq!(
            verify_factory_attestation(
                &attested,
                factory.public_key(),
                GenuineChallenge([4u8; 32]),
                device_id,
                &ds_signature
            ),
            Err(GenuineError::FactoryAttestationSignatureInvalid),
        );
        assert_eq!(
            verify_factory_attestation(
                &attested,
                factory.public_key(),
                challenge,
                other_id,
                &ds_signature
            ),
            Err(GenuineError::DeviceIdMismatch),
        );
        let relabelled = AttestedDevice::V0 {
            device_id: other_id,
            certificate: attested.certificate().clone(),
        };
        assert_eq!(
            verify_factory_attestation(
                &relabelled,
                factory.public_key(),
                challenge,
                other_id,
                &ds_signature
            ),
            Err(GenuineError::FactoryAttestationSignatureInvalid),
        );
        let other_factory = KeyPair::new_xonly(Scalar::random(&mut test_rng));
        assert_eq!(
            verify_factory_attestation(
                &attested,
                other_factory.public_key(),
                challenge,
                device_id,
                &ds_signature
            ),
            Err(GenuineError::UnknownFactoryKey),
        );
    }

    #[test]
    fn identity_attestation_is_bound_to_key_challenge_and_digest() {
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
        let device_keypair = keypair(5);
        let device_id = DeviceId::new(device_keypair.public_key());
        let challenge = GenuineChallenge([9u8; 32]);
        let digest = AttestedDeviceDigest([6u8; 32]);

        let signature = sign_identity_attestation(&schnorr, &device_keypair, challenge, digest);
        assert_eq!(
            verify_identity_attestation(device_id, challenge, digest, &signature),
            Ok(())
        );
        assert_eq!(
            verify_identity_attestation(device_id, GenuineChallenge([1u8; 32]), digest, &signature),
            Err(GenuineError::IdentityAttestationSignatureInvalid),
        );
        assert_eq!(
            verify_identity_attestation(
                device_id,
                challenge,
                AttestedDeviceDigest([7u8; 32]),
                &signature
            ),
            Err(GenuineError::IdentityAttestationSignatureInvalid),
        );
        let forged = sign_identity_attestation(&schnorr, &keypair(6), challenge, digest);
        assert_eq!(
            verify_identity_attestation(device_id, challenge, digest, &forged),
            Err(GenuineError::IdentityAttestationSignatureInvalid),
        );
        assert_eq!(
            verify_identity_attestation(DeviceId([0u8; 33]), challenge, digest, &signature),
            Err(GenuineError::IdentityAttestationSignatureInvalid),
        );
    }
}
