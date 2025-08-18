use frostsnap_comms::factory::Certificate;
use frostsnap_core::schnorr_fun::fun::marker::{NonZero, Secret};
use frostsnap_core::schnorr_fun::fun::{KeyPair, Scalar};
use frostsnap_core::schnorr_fun::Message;
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::RsaPublicKey;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use crate::FACTORY_KEY;

pub fn generate(
    rsa_public_key: &RsaPublicKey,
    serial_number: u32,
    case_color: String,
) -> Certificate {
    let certificate = {
        let pem_bytes = rsa_public_key.to_pkcs1_der().unwrap().to_vec();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let factory_secret = Scalar::<Secret, NonZero>::from_bytes(FACTORY_KEY).unwrap();
        let factory_keypair = KeyPair::new_xonly(factory_secret);
        let schnorr = frostsnap_core::schnorr_fun::new_with_synthetic_nonces::<
            sha2::Sha256,
            rand::rngs::ThreadRng,
        >();

        let message = Message::new("frostsnap-genuine-key", &pem_bytes);
        let signature = schnorr.sign(&factory_keypair, message);

        Certificate {
            rsa_key: pem_bytes,
            serial_number,
            timestamp,
            case_color,
            signature,
            factory_key: factory_keypair.public_key(),
        }
    };

    certificate
}

pub fn verify(certificate: &Certificate) -> bool {
    let message = Message::new("frostsnap-genuine-key", &certificate.rsa_key);
    let schnorr = frostsnap_core::schnorr_fun::new_with_synthetic_nonces::<
        sha2::Sha256,
        rand::rngs::ThreadRng,
    >();
    schnorr.verify(&certificate.factory_key, message, &certificate.signature)
}
