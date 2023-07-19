#![allow(non_snake_case)]
use alloc::collections::BTreeSet;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use rand_core::RngCore;
use schnorr_fun::frost::Frost;
use schnorr_fun::fun::{g, marker::*, Point, Scalar, G};
use schnorr_fun::nonce::NonceGen;
use sha2::{
    digest::{Digest, Update},
    Sha256,
};

use crate::DeviceId;
#[derive(Clone, Copy, Debug, Eq, PartialEq, bincode::Encode, bincode::Decode)]
pub struct EncryptedShare {
    R: Point,
    e: [u8; 32],
}

impl EncryptedShare {
    pub fn new(public_key: Point, rng: &mut impl RngCore, share: &Scalar<Secret, Zero>) -> Self {
        let r = Scalar::random(rng);
        let R = g!(r * G).normalize();
        let shared_secret_point = g!(r * public_key).normalize();
        let cipher_key = Sha256::default()
            .chain(shared_secret_point.to_bytes())
            .finalize();
        let mut cipher = ChaCha20::new(&cipher_key, &[0u8; 12].into());
        let mut e = share.to_bytes();
        cipher.apply_keystream(&mut e);
        EncryptedShare { R, e }
    }

    pub fn decrypt(mut self, secret_key: &Scalar) -> Scalar<Secret, Zero> {
        let shared_secret_point = g!(secret_key * self.R).normalize();
        let cipher_key = Sha256::default()
            .chain(shared_secret_point.to_bytes())
            .finalize();
        let mut cipher = ChaCha20::new(&cipher_key, &[0u8; 12].into());
        cipher.apply_keystream(&mut self.e);
        Scalar::from_bytes_mod_order(self.e)
    }
}

impl crate::KeyGenProvideShares {
    pub fn generate(
        frost: &Frost<sha2::Sha256, impl NonceGen>,
        my_poly: &[Scalar],
        devices: &BTreeSet<DeviceId>,
        secure_rng: &mut impl rand_core::RngCore,
    ) -> Self {
        let pop_message = crate::gen_pop_message(devices.clone());
        let proof_of_possession =
            frost.create_proof_of_possession(&my_poly, schnorr_fun::Message::raw(&pop_message));

        let encrypted_shares = devices
            .iter()
            .map(|&device| {
                let share = frost.create_share(&my_poly, device.to_poly_index());
                (
                    device,
                    EncryptedShare::new(device.pubkey, secure_rng, &share),
                )
            })
            .collect();

        Self {
            my_poly: schnorr_fun::frost::to_point_poly(&my_poly),
            proof_of_possession,
            encrypted_shares,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;
    use schnorr_fun::fun::s;

    #[test]
    fn encryption_roundtrip() {
        let sk = s!(1337);
        let pk = g!(sk * G).normalize();
        let share = s!(42).mark_zero();
        let mut rng = ChaCha20Rng::from_seed([12u8; 32]);

        let ciphertext = EncryptedShare::new(pk, &mut rng, &share);
        let decrypted = ciphertext.decrypt(&sk);

        assert_eq!(decrypted, s!(42));
    }
}
