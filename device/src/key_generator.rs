use esp_hal::hmac::{Error, Hmac, HmacPurpose, KeyId};
// TODO: Use the HMAC peripheral
use frostsnap_core::{device::DeviceSymmetricKeyGen, schnorr_fun::frost::PartyIndex, SymmetricKey};
use nb::block;

// #[derive(Debug, Default)]
pub struct HmacKeyGen<'a> {
    hmac: Hmac<'a>,
}

impl<'a> HmacKeyGen<'a> {
    pub fn new(hmac: Hmac<'a>) -> Self {
        Self { hmac }
    }

    pub fn hash(&mut self, input: &[u8], key_id: KeyId) -> Result<[u8; 32], Error> {
        let hmac = &mut self.hmac;

        let mut output = [0u8; 32];
        let mut remaining = input;

        hmac.init();
        block!(hmac.configure(HmacPurpose::ToUser, key_id))?;
        while !remaining.is_empty() {
            remaining = block!(hmac.update(remaining)).unwrap();
        }
        block!(hmac.finalize(output.as_mut_slice())).unwrap();

        Ok(output)
    }
}

impl DeviceSymmetricKeyGen for HmacKeyGen<'_> {
    fn get_share_encryption_key(
        &mut self,
        key_id: frostsnap_core::KeyId,
        access_structure_id: frostsnap_core::AccessStructureId,
        party_index: PartyIndex,
        coord_key: frostsnap_core::CoordShareDecryptionContrib,
    ) -> SymmetricKey {
        let mut src = [0u8; 128];
        src[..32].copy_from_slice(key_id.to_bytes().as_slice());
        src[32..64].copy_from_slice(access_structure_id.to_bytes().as_slice());
        src[64..96].copy_from_slice(party_index.to_bytes().as_slice());
        src[96..128].copy_from_slice(coord_key.to_bytes().as_slice());

        let output = self.hash(&src, KeyId::Key0).unwrap();

        SymmetricKey(output)
    }
}
