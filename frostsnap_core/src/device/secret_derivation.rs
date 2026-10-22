use crate::symmetric_encryption::SymmetricKey;
use crate::{AccessStructureRef, CoordShareDecryptionContrib};
use schnorr_fun::frost::ShareIndex;

pub trait DeviceSecretDerivation {
    fn get_share_encryption_key(
        &mut self,
        access_structure_ref: AccessStructureRef,
        party_index: ShareIndex,
        coord_key: CoordShareDecryptionContrib,
    ) -> SymmetricKey;

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: crate::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32];
}

/// A PRF under a secret only the device holds: unpredictable without it, and the same for the
/// device's whole life, since every share-encryption key and nonce seed is derived from it.
pub trait DevicePrf {
    /// The PRF over the concatenation of `message`.
    fn prf(&mut self, message: &[&[u8]]) -> [u8; 32];

    /// The domain-separated PRF every device secret is derived with. Its framing is frozen: it
    /// decides every existing device's keys.
    fn prf_domain(&mut self, domain: &str, input: &[u8]) -> [u8; 32] {
        self.prf(&[&[domain.len() as u8], domain.as_bytes(), input])
    }
}

/// The byte layouts here decide every existing device's share-encryption keys and nonces.
impl<T: DevicePrf + ?Sized> DeviceSecretDerivation for T {
    fn get_share_encryption_key(
        &mut self,
        access_structure_ref: AccessStructureRef,
        party_index: ShareIndex,
        coord_key: CoordShareDecryptionContrib,
    ) -> SymmetricKey {
        let mut src = [0u8; 128];
        src[..32].copy_from_slice(access_structure_ref.key_id.to_bytes().as_slice());
        src[32..64].copy_from_slice(
            access_structure_ref
                .access_structure_id
                .to_bytes()
                .as_slice(),
        );
        src[64..96].copy_from_slice(party_index.to_bytes().as_slice());
        src[96..128].copy_from_slice(coord_key.to_bytes().as_slice());
        SymmetricKey(self.prf_domain("share-encryption", &src))
    }

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: crate::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32] {
        let mut input = [0u8; 52];
        input[..16].copy_from_slice(nonce_stream_id.to_bytes().as_slice());
        input[16..20].copy_from_slice(&index.to_be_bytes());
        input[20..52].copy_from_slice(seed_material);
        self.prf_domain("nonce-seed", &input)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::nonce_stream::NonceStreamId;
    use crate::{AccessStructureId, KeyId};
    use schnorr_fun::fun::s;
    use sha2::{Digest, Sha256};

    struct Sha256Prf;

    impl DevicePrf for Sha256Prf {
        fn prf(&mut self, message: &[&[u8]]) -> [u8; 32] {
            let mut hash = Sha256::new();
            for part in message {
                hash.update(part);
            }
            hash.finalize().into()
        }
    }

    /// Frozen: these bytes decide every existing device's keys and nonces, its DeviceId included. They
    /// were produced by the device's derivations before they moved into frostsnap_core.
    #[test]
    fn derivation_layouts_are_frozen() {
        let key = Sha256Prf.get_share_encryption_key(
            AccessStructureRef {
                key_id: KeyId([1; 32]),
                access_structure_id: AccessStructureId([2; 32]),
            },
            s!(3).public(),
            "0404040404040404040404040404040404040404040404040404040404040404"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            schnorr_fun::fun::hex::encode(&key.0),
            "672c8455d3a7f8d616032c3b4b47f5d822a0568ab6d5acd7f68ae4bde9a9f91c"
        );

        let seed = Sha256Prf.derive_nonce_seed(NonceStreamId([5; 16]), 0x0102_0304, &[6; 32]);
        assert_eq!(
            schnorr_fun::fun::hex::encode(&seed),
            "79e4faf9eb4b81066e5cc18a159165002c3cd9e340b5f412118ca58b3e992cc1"
        );

        let keypair_secret = Sha256Prf.prf_domain("frostsnap-device-keypair", &[7; 32]);
        assert_eq!(
            schnorr_fun::fun::hex::encode(&keypair_secret),
            "2f9ad85da12250e8566a33269e97db0b82ccd8cbc8d72a4c52878a2b24babfc3"
        );
    }
}
