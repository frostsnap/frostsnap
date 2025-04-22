use core::cell::RefCell;

use crate::ota::OtaPartitions;
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
}

impl<'a> Partitions<'a> {
    fn new(flash: &'a RefCell<FlashStorage>) -> Self {
        Self {
            ota: OtaPartitions {
                otadata: EspFlashPartition::new(flash, 0, 0, "otadata"),
                ota_0: EspFlashPartition::new(flash, 0, 0, "ota_0"),
                ota_1: EspFlashPartition::new(flash, 0, 0, "ota_1"),
                factory: EspFlashPartition::new(flash, 0, 0, "factory"),
            },
            nvs: EspFlashPartition::new(flash, 0, 0, "nvs"),
        }
    }
    pub fn load(flash: &'a RefCell<FlashStorage>) -> Self {
        let table = esp_partition_table::PartitionTable::new(0xb000, 10 * 32);

        let mut self_ = Self::new(flash);

        for row in table.iter_storage(&mut *flash.borrow_mut(), false) {
            let row = match row {
                Ok(row) => row,
                Err(_) => panic!("unable to read row of partition table"),
            };
            assert_eq!(row.offset % FlashStorage::ERASE_SIZE as u32, 0);
            match row.name() {
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
                "factory" => {
                    self_
                        .ota
                        .factory
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
            self_.ota.factory,
        ] {
            assert!(part.size() > 0, "partition {} must not me empty", part.tag);
        }

        self_
    }
}

pub trait PartitionExt {
    fn sha256_digest(&self, sha256: &mut Sha<'_>) -> Sha256Digest;
}

impl PartitionExt for EspFlashPartition<'_> {
    fn sha256_digest(&self, sha256: &mut esp_hal::sha::Sha<'_>) -> Sha256Digest {
        let mut digest = [0u8; 32];
        let mut hasher = sha256.start::<esp_hal::sha::Sha256>();
        for i in 0..self.n_sectors() {
            let sector = self.read_sector(i).unwrap();
            let mut remaining = &sector[..];
            while !remaining.is_empty() {
                remaining = nb::block!(hasher.update(remaining)).unwrap();
            }
        }

        nb::block!(hasher.finish(&mut digest)).unwrap();

        Sha256Digest(digest)
    }
}
