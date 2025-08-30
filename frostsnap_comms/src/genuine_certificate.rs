use crate::factory::Certificate;
use frostsnap_core::schnorr_fun::fun::marker::EvenY;
use frostsnap_core::schnorr_fun::fun::{rand_core, Point};
use frostsnap_core::schnorr_fun::Message;

pub fn verify<R: rand_core::RngCore + Default + Clone>(
    certificate: &Certificate,
    factory_key: Point<EvenY>,
) -> bool {
    if factory_key != certificate.factory_key {
        // TODO: return error of UnknownFactoryKey
        return false;
    }

    let message = Message::new("frostsnap-genuine-key", &certificate.rsa_key);
    let schnorr =
        frostsnap_core::schnorr_fun::new_with_synthetic_nonces::<frostsnap_core::sha2::Sha256, R>();
    schnorr.verify(&factory_key, message, &certificate.signature)
}
