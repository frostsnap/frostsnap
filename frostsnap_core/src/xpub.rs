use bitcoin::{secp256k1, util::bip32::*, Network};
use schnorr_fun::{
    frost::FrostKey,
    fun::{marker::*, Scalar},
};

/// Encapsulates bip32 derivations on a FROST key
pub struct FrostXpub {
    frost_key: FrostKey<Normal>,
    xpub: ExtendedPubKey,
}

impl FrostXpub {
    pub fn new(frost_key: FrostKey<Normal>) -> Self {
        FrostXpub {
            xpub: ExtendedPubKey {
                network: Network::Bitcoin,
                depth: 0,
                child_number: ChildNumber::from(0u32),
                parent_fingerprint: Fingerprint::default(),
                public_key: secp256k1::PublicKey::from_slice(
                    frost_key.public_key().to_bytes().as_ref(),
                )
                .unwrap(),
                chain_code: ChainCode::from([0u8; 32].as_ref()),
            },
            frost_key,
        }
    }

    /// Does non-hardended derivation
    pub fn derive_bip32(&mut self, segments: &[u32]) {
        for child in segments {
            let child_number = ChildNumber::Normal { index: *child };
            let (tweak, chain_code) = self
                .xpub
                .ckd_pub_tweak(child_number)
                .expect("can only fail if you do non-hardended derivation");
            self.frost_key = self
                .frost_key
                .clone()
                .tweak(
                    Scalar::<Public, _>::from_slice(&tweak[..])
                        .unwrap()
                        .non_zero()
                        .expect("sk cannot be zero"),
                )
                .expect("computationally unreachable");
            self.xpub = ExtendedPubKey {
                network: self.xpub.network,
                depth: self.xpub.depth + 1,
                parent_fingerprint: self.xpub.fingerprint(),
                child_number,
                public_key: secp256k1::PublicKey::from_slice(
                    self.frost_key.public_key().to_bytes().as_ref(),
                )
                .unwrap(),
                chain_code,
            }
        }
    }

    pub fn frost_key(&self) -> &FrostKey<Normal> {
        &self.frost_key
    }

    pub fn xpub(&self) -> &ExtendedPubKey {
        &self.xpub
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::vec::Vec;
    use bitcoin::secp256k1::Secp256k1;
    use schnorr_fun::frost;

    #[test]
    pub fn bip32_derivation_matches_rust_bitcoin() {
        let frost = frost::new_with_deterministic_nonces::<sha2::Sha256>();
        let (frost_key, _) = frost.simulate_keygen(3, 5, &mut rand::thread_rng());
        let mut frost_xpub = FrostXpub::new(frost_key);
        let secp = Secp256k1::verification_only();
        let xpub = frost_xpub.xpub();

        let path = [1337u32, 42, 0];
        let child_path = path
            .iter()
            .map(|i| ChildNumber::Normal { index: *i })
            .collect::<Vec<_>>();
        let derived_xpub = xpub.derive_pub(&secp, &child_path).unwrap();
        frost_xpub.derive_bip32(&path);

        assert_eq!(frost_xpub.xpub(), &derived_xpub);
    }
}
