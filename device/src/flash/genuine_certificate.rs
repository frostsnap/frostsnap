use crate::partitions::EspFlashPartition;
use alloc::vec::Vec;
use esp_storage::FlashStorage;
use frostsnap_comms::genuine_certificate::Certificate;
use frostsnap_core::Versioned;
use frostsnap_embedded::FlashPartition;
use frostsnap_embedded::ABWRITE_BINCODE_CONFIG;

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct VersionedFactoryData {
    inner: Versioned<FactoryData>,
}

#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct FactoryData {
    pub ds_encrypted_params: Vec<u8>,
    pub certificate: Certificate,
}

impl VersionedFactoryData {
    pub fn read<'a>(
        partition: FlashPartition<'a, FlashStorage<'static>>,
    ) -> Result<Self, bincode::error::DecodeError> {
        bincode::decode_from_reader::<VersionedFactoryData, _, _>(
            partition.bincode_reader(),
            ABWRITE_BINCODE_CONFIG,
        )
    }

    pub fn init(encrypted_params: Vec<u8>, certificate: Certificate) -> Self {
        Self {
            inner: Versioned::V0(FactoryData {
                ds_encrypted_params: encrypted_params,
                certificate,
            }),
        }
    }

    pub fn into_factory_data(self) -> FactoryData {
        match self.inner {
            Versioned::V0(factory_data) => factory_data,
        }
    }
}

/// The factory data in flash. Read it each time it's needed rather than keeping it in memory.
#[derive(Clone, Copy)]
pub struct FactoryDataHandle<'a>(EspFlashPartition<'a>);

impl<'a> FactoryDataHandle<'a> {
    /// `None` if the partition holds no factory data.
    pub fn open(partition: EspFlashPartition<'a>) -> Option<Self> {
        VersionedFactoryData::read(partition).ok()?;
        Some(Self(partition))
    }

    pub fn read(&self) -> Result<FactoryData, bincode::error::DecodeError> {
        VersionedFactoryData::read(self.0).map(VersionedFactoryData::into_factory_data)
    }
}
