use alloc::vec::Vec;
use esp_storage::FlashStorage;
use frostsnap_comms::factory::Certificate;
use frostsnap_core::Versioned;
use frostsnap_embedded::FlashPartition;
use frostsnap_embedded::ABWRITE_BINCODE_CONFIG;

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct FactoryData {
    inner: Versioned<FactoryDataContents>,
}

impl FactoryData {
    pub fn read<'a>(
        partition: FlashPartition<'a, FlashStorage>,
    ) -> Result<Self, bincode::error::DecodeError> {
        bincode::decode_from_reader::<FactoryData, _, _>(
            partition.bincode_reader(),
            ABWRITE_BINCODE_CONFIG,
        )
    }

    pub fn init(encrypted_params: Vec<u8>, certificate: Certificate) -> Self {
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
