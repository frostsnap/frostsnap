//! Portable device secret derivation over the `KeyedHash` seam.
//!
//! `DeviceSecretDerivation` is a pure function of `keyed_hash(domain, input)`: it
//! packs typed fields into a fixed byte layout and hashes. That layout — the
//! 128-byte `share-encryption` source, the `nonce-seed` input including
//! `index.to_be_bytes()` — is **platform-independent** and must live in exactly one
//! place. Only the keyed-hash *primitive* is per-platform (esp: the HMAC peripheral
//! over an eFuse key; host: software HMAC-SHA256 over a RAM key), and that is what
//! [`KeyedHash`] already captures. Both `device/` and the sim wrap their own
//! `KeyedHash` in `ShareEncryptionSecrets`, so sim/hardware fidelity is a
//! compile-time fact rather than a copy-paste convention that could drift on the
//! safety-critical nonce-derivation path.

use crate::flash_header::KeyedHash;
use frostsnap_core::{
    device::DeviceSecretDerivation, nonce_stream::NonceStreamId, schnorr_fun::frost::ShareIndex,
    AccessStructureRef, CoordShareDecryptionContrib, SymmetricKey,
};

/// Wraps a [`KeyedHash`] primitive with the portable share-encryption / nonce
/// derivation. `H` is the only thing that varies by platform.
pub struct ShareEncryptionSecrets<H: KeyedHash>(pub H);

impl<H: KeyedHash> DeviceSecretDerivation for ShareEncryptionSecrets<H> {
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

        SymmetricKey(self.0.keyed_hash("share-encryption", &src))
    }

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32] {
        let mut input = [0u8; 52]; // 16 (stream_id) + 4 (index) + 32 (seed_material)
        input[..16].copy_from_slice(nonce_stream_id.to_bytes().as_slice());
        input[16..20].copy_from_slice(&index.to_be_bytes());
        input[20..52].copy_from_slice(seed_material);

        self.0.keyed_hash("nonce-seed", &input)
    }
}
