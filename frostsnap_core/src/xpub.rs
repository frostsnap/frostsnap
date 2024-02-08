use bitcoin::{bip32::*, secp256k1, Network};
use schnorr_fun::{
    frost::FrostKey,
    fun::{g, marker::*, Point, Scalar, G},
};

/// Encapsulates bip32 derivations on a key
pub struct Xpub<T> {
    key: T,
    xpub: ExtendedPubKey,
}

pub trait TweakableKey: Clone {
    type XOnly;
    fn to_libsecp_key(&self) -> secp256k1::PublicKey;
    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey;
    fn tweak(self, tweak: Scalar<Public>) -> Self;
    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly;
    fn into_xonly(self) -> Self::XOnly;
}

impl TweakableKey for FrostKey<Normal> {
    type XOnly = FrostKey<EvenY>;

    fn to_libsecp_key(&self) -> secp256k1::PublicKey {
        self.public_key().to_libsecp_key()
    }

    fn tweak(self, tweak: Scalar<Public>) -> Self {
        FrostKey::<Normal>::tweak(self, tweak).expect("computationally unreachable")
    }

    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly {
        self.into_xonly_key()
            .tweak(tweak)
            .expect("if tweak is a hash this should be unreachable")
    }

    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey {
        self.public_key().to_libsecp_xonly()
    }

    fn into_xonly(self) -> Self::XOnly {
        self.into_xonly_key()
    }
}

impl TweakableKey for Point {
    type XOnly = Point<EvenY>;

    fn to_libsecp_key(&self) -> secp256k1::PublicKey {
        secp256k1::PublicKey::from_slice(self.to_bytes().as_ref()).unwrap()
    }

    fn tweak(self, tweak: Scalar<Public>) -> Self {
        g!(self + tweak * G)
            .normalize()
            .non_zero()
            .expect("if tweak is a hash this should be unreachable")
    }

    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly {
        let (even_y, _) = self.into_point_with_even_y();
        let (tweaked_even_y, _) = g!(even_y + tweak * G)
            .normalize()
            .non_zero()
            .expect("if tweak is a hash this should be unreachable")
            .into_point_with_even_y();
        tweaked_even_y
    }

    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey {
        secp256k1::XOnlyPublicKey::from_slice(self.to_xonly_bytes().as_ref()).unwrap()
    }

    fn into_xonly(self) -> Self::XOnly {
        let (even_y, _) = self.into_point_with_even_y();
        even_y
    }
}

impl<T: TweakableKey> Xpub<T> {
    pub fn new(key: T) -> Self {
        Self {
            xpub: ExtendedPubKey {
                network: Network::Bitcoin,
                depth: 0,
                child_number: ChildNumber::from(0u32),
                parent_fingerprint: Fingerprint::default(),
                public_key: key.to_libsecp_key(),
                chain_code: ChainCode::from([0u8; 32]),
            },
            key,
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
            self.key = self.key.clone().tweak(
                Scalar::<Public, _>::from_slice(&tweak[..])
                    .unwrap()
                    .non_zero()
                    .expect("sk cannot be zero"),
            );
            self.xpub = ExtendedPubKey {
                network: self.xpub.network,
                depth: self.xpub.depth + 1,
                parent_fingerprint: self.xpub.fingerprint(),
                child_number,
                public_key: self.key.to_libsecp_key(),
                chain_code,
            }
        }
    }

    pub fn key(&self) -> &T {
        &self.key
    }

    pub fn into_key(self) -> T {
        self.key
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
        let mut frost_xpub = Xpub::new(frost_key);
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
