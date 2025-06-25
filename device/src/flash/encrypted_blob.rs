use alloc::vec::Vec;
use frostsnap_comms::factory::Certificate;
use frostsnap_core::Versioned;

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct FactoryData {
    inner: Versioned<FactoryDataContents>,
}

impl FactoryData {
    pub fn new(encrypted_params: Vec<u8>, certificate: Certificate) -> Self {
        Self {
            inner: Versioned::V0(FactoryDataContents {
                encrypted_params,
                certificate,
            }),
        }
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct FactoryDataContents {
    pub encrypted_params: Vec<u8>,
    pub certificate: Certificate,
}
