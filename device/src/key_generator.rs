// TODO: Use the HMAC peripheral
use frostsnap_core::{device::DeviceSymmetricKeyGen, schnorr_fun::frost::PartyIndex, SymmetricKey};

#[derive(Debug, Default)]
pub struct HmacKeyGen;

impl HmacKeyGen {
    pub fn new() -> Self {
        Self
    }
}

impl DeviceSymmetricKeyGen for HmacKeyGen {
    fn get_share_encryption_key(
        &mut self,
        _key_id: frostsnap_core::KeyId,
        _access_structure_id: frostsnap_core::AccessStructureId,
        _party_index: PartyIndex,
        _coord_key: frostsnap_core::CoordShareDecryptionContrib,
    ) -> SymmetricKey {
        SymmetricKey([42u8; 32])
    }
}
