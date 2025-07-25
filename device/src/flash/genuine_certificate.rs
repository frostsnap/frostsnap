use alloc::vec::Vec;
use frostsnap_comms::factory::Certificate;
use frostsnap_core::Versioned;

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
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

    pub fn certificate(&self) -> Certificate {
        match &self.inner {
            Versioned::V0(contents) => contents.certificate.clone(),
        }
    }

    pub fn encrypted_params(&self) -> Vec<u8> {
        match &self.inner {
            Versioned::V0(contents) => contents.encrypted_params.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct FactoryDataContents {
    pub encrypted_params: Vec<u8>,
    pub certificate: Certificate,
}
