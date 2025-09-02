use crate::firmware_size::FirmwareSizeError;
use crate::ota::OtaPartitions;
use core::cell::RefCell;
use embedded_storage::nor_flash::NorFlash;
use esp_hal::sha::Sha;
use esp_storage::FlashStorage;
use frostsnap_comms::Sha256Digest;
use frostsnap_embedded::FlashPartition;

pub type EspFlashPartition<'a> = FlashPartition<'a, FlashStorage>;

#[derive(Clone)]
pub struct Partitions<'a> {
    pub ota: OtaPartitions<'a>,
    pub nvs: EspFlashPartition<'a>,
    pub factory_data: EspFlashPartition<'a>,
}

impl<'a> Partitions<'a> {
    fn new(flash: &'a RefCell<FlashStorage>) -> Self {
        Self {
            ota: OtaPartitions {
                otadata: EspFlashPartition::new(flash, 0, 0, "otadata"),
                ota_0: EspFlashPartition::new(flash, 0, 0, "ota_0"),
                ota_1: EspFlashPartition::new(flash, 0, 0, "ota_1"),
            },
            nvs: EspFlashPartition::new(flash, 0, 0, "nvs"),
            factory_data: EspFlashPartition::new(flash, 0, 0, "factory_cert"),
        }
    }

    pub fn load(flash: &'a RefCell<FlashStorage>) -> Self {
        let table = esp_partition_table::PartitionTable::new(0xd000, 10 * 32);

        let mut self_ = Self::new(flash);
        for row in table.iter_storage(&mut *flash.borrow_mut(), false) {
            let row = match row {
                Ok(row) => row,
                Err(_) => panic!("unable to read row of partition table"),
            };
            assert_eq!(row.offset % FlashStorage::ERASE_SIZE as u32, 0);
            match row.name() {
                "factory_cert" => {
                    self_
                        .factory_data
                        .set_offset_and_size(row.offset, row.size as u32);
                }
                "otadata" => {
                    self_
                        .ota
                        .otadata
                        .set_offset_and_size(row.offset, row.size as u32);
                }
                "ota_0" => {
                    self_
                        .ota
                        .ota_0
                        .set_offset_and_size(row.offset, row.size as u32);
                }
                "ota_1" => {
                    self_
                        .ota
                        .ota_1
                        .set_offset_and_size(row.offset, row.size as u32);
                }
                "nvs" => {
                    self_.nvs.set_offset_and_size(row.offset, row.size as u32);
                }
                _ => { /*ignore*/ }
            }
        }
        for part in [
            self_.nvs,
            self_.ota.otadata,
            self_.ota.ota_0,
            self_.ota.ota_1,
            self_.factory_data,
        ] {
            assert!(part.size() > 0, "partition {} must not be empty", part.tag);
        }
        self_
    }
}

pub trait PartitionExt {
    /// Calculate SHA256 digest of partition data
    ///
    /// # Arguments
    /// * `sha256` - SHA256 hardware peripheral
    /// * `up_to` - Optional byte limit.
    fn sha256_digest(&self, sha256: &mut Sha<'_>, up_to: Option<u32>) -> Sha256Digest;
    fn firmware_size(&self) -> Result<u32, FirmwareSizeError>;
}

impl PartitionExt for EspFlashPartition<'_> {
    fn sha256_digest(
        &self,
        sha256: &mut esp_hal::sha::Sha<'_>,
        up_to: Option<u32>,
    ) -> Sha256Digest {
        let mut digest = [0u8; 32];
        let mut hasher = sha256.start::<esp_hal::sha::Sha256>();
        let mut bytes_hashed = 0u32;

        // Calculate how many bytes to hash
        let bytes_to_hash_total = match up_to {
            Some(limit) => limit.min(self.size()),
            None => self.size(),
        };

        for i in 0..self.n_sectors() {
            if bytes_hashed >= bytes_to_hash_total {
                break;
            }
            let sector = self.read_sector(i).unwrap();
            let bytes_to_hash =
                (bytes_to_hash_total - bytes_hashed).min(sector.len() as u32) as usize;
            let mut remaining = &sector[..bytes_to_hash];
            while !remaining.is_empty() {
                remaining = nb::block!(hasher.update(remaining)).unwrap();
            }
            bytes_hashed += bytes_to_hash as u32;
        }
        nb::block!(hasher.finish(&mut digest)).unwrap();
        Sha256Digest(digest)
    }

    fn firmware_size(&self) -> Result<u32, FirmwareSizeError> {
        crate::firmware_size::firmware_size(self)
    }
}
