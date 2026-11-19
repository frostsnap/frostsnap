use bitcoin::hashes::hmac::{Hmac, HmacEngine};
use bitcoin::hashes::{Hash, HashEngine, sha256};
use frostsnap_core::device::DeviceSecretDerivation;
use frostsnap_core::schnorr_fun::frost::ShareIndex;
use frostsnap_core::schnorr_fun::fun::KeyPair;
use frostsnap_core::sha2::{Digest, Sha256};
use frostsnap_core::{AccessStructureRef, CoordShareDecryptionContrib, SymmetricKey};

pub struct SimDeviceKeyGen {
    secret: [u8; 32],
}

impl SimDeviceKeyGen {
    pub fn new(keypair: &KeyPair) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"frostsnap-sim-device-secret-key");
        hasher.update(keypair.secret_key().to_bytes());
        let secret = hasher.finalize().into();
        Self { secret }
    }
}

impl DeviceSecretDerivation for SimDeviceKeyGen {
    fn get_share_encryption_key(
        &mut self,
        _access_structure_ref: AccessStructureRef,
        _party_index: ShareIndex,
        _coord_key: CoordShareDecryptionContrib,
    ) -> SymmetricKey {
        SymmetricKey(self.secret)
    }

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: frostsnap_core::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32] {
        let mut engine = HmacEngine::<sha256::Hash>::new(&self.secret);
        engine.input(nonce_stream_id.to_bytes().as_slice());
        engine.input(&index.to_le_bytes());
        engine.input(seed_material);
        *Hmac::<sha256::Hash>::from_engine(engine).as_byte_array()
    }
}
