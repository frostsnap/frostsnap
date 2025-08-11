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
        let table = esp_partition_table::PartitionTable::new(0x8000, 10 * 32);

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
    fn sha256_digest(&self, sha256: &mut Sha<'_>, firmware_size: u32) -> Sha256Digest;
    fn firmware_size(&self) -> u32;
}

impl PartitionExt for EspFlashPartition<'_> {
    fn sha256_digest(
        &self,
        sha256: &mut esp_hal::sha::Sha<'_>,
        firmware_size: u32,
    ) -> Sha256Digest {
        let mut digest = [0u8; 32];
        let mut hasher = sha256.start::<esp_hal::sha::Sha256>();
        let mut bytes_hashed = 0u32;

        for i in 0..self.n_sectors() {
            if bytes_hashed >= firmware_size {
                break;
            }
            let sector = self.read_sector(i).unwrap();
            let bytes_to_hash = (firmware_size - bytes_hashed).min(sector.len() as u32) as usize;

            let mut remaining = &sector[..bytes_to_hash];
            while !remaining.is_empty() {
                remaining = nb::block!(hasher.update(remaining)).unwrap();
            }
            bytes_hashed += bytes_to_hash as u32;
        }

        nb::block!(hasher.finish(&mut digest)).unwrap();
        Sha256Digest(digest)
    }

    // fn sha256_digest(&self, sha256: &mut esp_hal::sha::Sha<'_>) -> Sha256Digest {
    //     let mut digest = [0u8; 32];
    //     let mut hasher = sha256.start::<esp_hal::sha::Sha256>();
    //     for i in 0..self.n_sectors() {
    //         let sector = self.read_sector(i).unwrap();
    //         let mut remaining = &sector[..];
    //         while !remaining.is_empty() {
    //             remaining = nb::block!(hasher.update(remaining)).unwrap();
    //         }
    //     }

    //     nb::block!(hasher.finish(&mut digest)).unwrap();

    //     Sha256Digest(digest)
    // }

    // From: https://github.com/esp-rs/espflash/blob/main/espflash/src/image_format/idf.rs
    fn firmware_size(&self) -> u32 {
        const ESP_MAGIC: u8 = 0xE9;

        #[derive(Debug, Clone, Copy)]
        #[repr(C, packed)]
        struct ImageHeader {
            magic: u8,
            segment_count: u8,
            flash_mode: u8,
            flash_config: u8,
            entry: u32,
            // extended header part
            wp_pin: u8,
            clk_q_drv: u8,
            d_cs_drv: u8,
            gd_wp_drv: u8,
            chip_id: u16,
            min_rev: u8,
            min_chip_rev_full: u16,
            max_chip_rev_full: u16,
            reserved: [u8; 4],
            append_digest: u8,
        }

        #[derive(Debug, Clone, Copy)]
        #[repr(C, packed)]
        struct SegmentHeader {
            addr: u32,
            length: u32,
        }

        let first_sector = self.read_sector(0).unwrap();

        // Use espflash's proven header parsing
        let header =
            unsafe { core::ptr::read_unaligned(first_sector.as_ptr() as *const ImageHeader) };

        if header.magic != ESP_MAGIC {
            panic!("Invalid firmware header magic");
        }

        // Use your proven layout logic, espflash structs
        let mut current_pos = 24u32; // After header
        let mut max_data_end = 24u32;

        for _ in 0..header.segment_count {
            let sector_num = (current_pos / 4096) as u32;
            let sector_offset = (current_pos % 4096) as usize;

            let sector = if sector_num == 0 {
                &first_sector
            } else {
                &self.read_sector(sector_num).unwrap()
            };

            // Use espflash's struct instead of manual byte extraction
            let seg_header = if sector_offset + 8 <= sector.len() {
                unsafe {
                    core::ptr::read_unaligned(
                        sector.as_ptr().add(sector_offset) as *const SegmentHeader
                    )
                }
            } else {
                // Handle spanning (keep your logic)
                let mut seg_bytes = [0u8; 8];
                let first_part = sector.len() - sector_offset;
                seg_bytes[..first_part].copy_from_slice(&sector[sector_offset..]);

                let next_sector = self.read_sector(sector_num + 1).unwrap();
                seg_bytes[first_part..].copy_from_slice(&next_sector[..8 - first_part]);

                unsafe { core::ptr::read_unaligned(seg_bytes.as_ptr() as *const SegmentHeader) }
            };

            let segment_data_end = current_pos + 8 + seg_header.length;
            max_data_end = max_data_end.max(segment_data_end);
            current_pos += 8 + seg_header.length;
        }

        // Use espflash's exact padding logic
        let mut firmware_end = (max_data_end + 15) & !15;

        // Use header flag instead of hardcoded check
        if header.append_digest == 1 {
            firmware_end += 32;
        }

        // Check for signatures without large buffer allocation
        let signature_search_start = firmware_end;
        let search_sectors = 2u32; // Only search 2 sectors (8KB) for signatures

        for i in 0..search_sectors {
            let sector_start = signature_search_start + (i * 4096);
            if sector_start >= self.size() {
                break;
            }

            let sector_num = sector_start / 4096;
            if sector_num >= self.n_sectors() as u32 {
                break;
            }

            let sector = self.read_sector(sector_num).unwrap();

            // Search for signature magic in this sector
            for pos in 0..sector.len().saturating_sub(4) {
                if sector[pos] == 0xE7
                    && sector[pos + 1] == 0x02
                    && sector[pos + 2] == 0x00
                    && sector[pos + 3] == 0x00
                {
                    // Found signature - return position relative to partition start
                    return sector_start + pos as u32;
                }
            }
        }

        firmware_end
    }
}
