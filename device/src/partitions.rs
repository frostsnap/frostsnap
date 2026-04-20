use crate::ota::OtaPartitions;
use core::cell::RefCell;
use embedded_storage::nor_flash::NorFlash;
use esp_bootloader_esp_idf::partitions;
use esp_hal::sha::Sha;
use esp_storage::FlashStorage;
use frostsnap_comms::firmware_reader::FirmwareSizeError;
use frostsnap_comms::Sha256Digest;
use frostsnap_embedded::FlashPartition;

pub type EspFlashPartition<'a> = FlashPartition<'a, FlashStorage>;

#[derive(Clone)]
pub struct Partitions<'a> {
    pub factory_cert: EspFlashPartition<'a>,
    pub ota: OtaPartitions<'a>,
    pub nvs: EspFlashPartition<'a>,
}

impl<'a> Partitions<'a> {
    fn new(flash: &'a RefCell<FlashStorage>) -> Self {
        Self {
            factory_cert: EspFlashPartition::new(flash, 0, 0, "factory_cert"),
            ota: OtaPartitions {
                otadata: EspFlashPartition::new(flash, 0, 0, "otadata"),
                ota_0: EspFlashPartition::new(flash, 0, 0, "ota_0"),
                ota_1: EspFlashPartition::new(flash, 0, 0, "ota_1"),
            },
            nvs: EspFlashPartition::new(flash, 0, 0, "nvs"),
        }
    }

    pub fn load(flash: &'a RefCell<FlashStorage>) -> Self {
        let mut self_ = Self::new(flash);
        let mut pt_mem = [0u8; partitions::PARTITION_TABLE_MAX_LEN];
        // Partition table offset in .cargo/config.toml env
        // ESP_BOOTLOADER_ESP_IDF_CONFIG_PARTITION_TABLE_OFFSET = "0xD000"
        let pt = partitions::read_partition_table(&mut *flash.borrow_mut(), &mut pt_mem)
            .expect("unable to read partition table");

        for i in 0..pt.len() {
            let row = pt
                .get_partition(i)
                .expect("partition table index should be valid");
            assert_eq!(row.offset() % FlashStorage::ERASE_SIZE as u32, 0);
            match row.label_as_str() {
                "factory_cert" => {
                    self_
                        .factory_cert
                        .set_offset_and_size(row.offset(), row.len());
                }
                "otadata" => {
                    self_
                        .ota
                        .otadata
                        .set_offset_and_size(row.offset(), row.len());
                }
                "ota_0" => {
                    self_.ota.ota_0.set_offset_and_size(row.offset(), row.len());
                }
                "ota_1" => {
                    self_.ota.ota_1.set_offset_and_size(row.offset(), row.len());
                }
                "nvs" => {
                    self_.nvs.set_offset_and_size(row.offset(), row.len());
                }
                _ => { /*ignore*/ }
            }
        }
        for part in [
            self_.factory_cert,
            self_.ota.otadata,
            self_.ota.ota_0,
            self_.ota.ota_1,
            self_.nvs,
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

    /// Calculate firmware size information
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - Firmware content size (without signature blocks)
    /// - Total size including signature blocks if present
    fn firmware_size(&self) -> Result<(u32, u32), FirmwareSizeError>;
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

    fn firmware_size(&self) -> Result<(u32, u32), FirmwareSizeError> {
        frostsnap_comms::firmware_reader::firmware_size(self)
    }
}
