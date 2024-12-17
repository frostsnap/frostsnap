use crate::{
    io::SerialIo,
    ui::{self, UserInteraction},
};
use alloc::boxed::Box;
use embedded_storage::{nor_flash, ReadStorage, Storage};
use esp_hal::{sha::Sha, uart, Blocking};
use esp_storage::FlashStorage;
use frostsnap_comms::{
    DeviceSendBody, FirmwareDigest, BAUDRATE, FIRMWARE_IMAGE_SIZE,
    FIRMWARE_NEXT_CHUNK_READY_SIGNAL, FIRMWARE_UPGRADE_CHUNK_LEN,
};
use nb::block;

/// CRC used by out bootloader (and incidentally python's binutils crc32 function when passed 0xFFFFFFFF as the init).
const CRC: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::Algorithm {
    width: 32,
    poly: 0x04c11db7,
    init: 0x0,
    refin: true,
    refout: true,
    xorout: 0xffffffff,
    check: 0xcbf43926, // This is just for reference
    residue: 0xdebb20e3,
});

const SECTOR_SIZE: u32 = 4096;
const ESP32_OTADATA_SIZE: u32 = 32;
const FS_PARTITION_METADATA_SIZE: u32 = 256;
const SECTORS_PER_IMAGE: u32 = FIRMWARE_IMAGE_SIZE / SECTOR_SIZE;
/// We switch the baudrate during OTA update to make it faster
const OTA_UPDATE_BAUD: u32 = 921_600;

#[derive(Debug, Clone, Copy)]
pub struct OtaConfig {
    otadata_offset: u32,
    factory_partition: Partition,
    ota_partitions: [Partition; 2],
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Partition {
    offset: u32,
    size: u32,
}

impl Partition {
    pub fn erase_image_sector(&self, flash: &mut FlashStorage, sector: u32) {
        if sector >= SECTORS_PER_IMAGE {
            panic!("tried to erase sector out of bounds");
        }
        let start = self.offset + sector * SECTOR_SIZE;
        nor_flash::NorFlash::erase(flash, start, start + SECTOR_SIZE).unwrap();
    }

    pub fn digest(
        &self,
        flash: &mut FlashStorage,
        sha256: &mut Sha<'_, Blocking>,
    ) -> FirmwareDigest {
        for i in 0..(self.size / SECTOR_SIZE) {
            let sector = self.get_sector(flash, i).unwrap();
            let mut remaining = &sector[..];
            while !remaining.is_empty() {
                remaining = block!(sha256.update(remaining)).unwrap();
            }
        }
        let mut digest = FirmwareDigest([0u8; 32]);
        block!(sha256.finish(&mut digest.0)).unwrap();
        digest
    }

    pub fn get_sector(
        &self,
        flash: &mut FlashStorage,
        sector: u32,
    ) -> Result<[u8; SECTOR_SIZE as usize], &'static str> {
        if sector >= SECTORS_PER_IMAGE {
            return Err("requested sector is out of bounds");
        }
        let mut ret = [0u8; SECTOR_SIZE as usize];
        flash
            .read(self.offset + sector * SECTOR_SIZE, &mut ret)
            .unwrap();
        Ok(ret)
    }

    pub fn write_sector(
        &self,
        flash: &mut FlashStorage,
        sector: u32,
        bytes: &[u8; SECTOR_SIZE as usize],
    ) -> Result<(), &'static str> {
        if sector >= SECTORS_PER_IMAGE {
            return Err("can't write sector out of bounds");
        }
        nor_flash::NorFlash::write(flash, self.offset + sector * SECTOR_SIZE, &bytes[..])
            .expect("hardware specific error");
        Ok(())
    }
}

impl OtaConfig {
    pub fn new(flash: &mut impl ReadStorage) -> Self {
        let table = esp_partition_table::PartitionTable::new(0x8000, 10 * 32);
        let mut otadata_offset = 0;
        let mut ota_partitions = [Partition::default(); 2];
        let mut factory_partition = Partition::default();

        for row in table.iter_storage(flash, false) {
            let row = match row {
                Ok(row) => row,
                Err(_) => panic!("unable to read row of partition table"),
            };
            match row.name() {
                "otadata" => {
                    otadata_offset = row.offset;
                }
                "ota_0" => {
                    ota_partitions[0] = Partition {
                        offset: row.offset,
                        size: row.size as u32,
                    };
                }
                "ota_1" => {
                    ota_partitions[1] = Partition {
                        offset: row.offset,
                        size: row.size as u32,
                    };
                }
                "factory" => {
                    factory_partition = Partition {
                        offset: row.offset,
                        size: row.size as u32,
                    };
                }
                _ => { /*ignore*/ }
            }
        }

        assert!(ota_partitions
            .iter()
            .all(|parition| parition != &Default::default()));
        assert_ne!(otadata_offset, Default::default());
        assert_ne!(factory_partition, Default::default());

        Self {
            otadata_offset,
            ota_partitions,
            factory_partition,
        }
    }

    fn next_slot(&self, flash: &mut FlashStorage) -> u8 {
        self.current_slot(flash)
            .map(|(i, _, _)| (i + 1) % 2)
            .unwrap_or(0)
    }

    pub fn active_partition(&self, flash: &mut FlashStorage) -> Partition {
        match self.current_slot(flash) {
            Some((slot, _, _)) => self.ota_partitions[slot as usize],
            None => self.factory_partition,
        }
    }

    pub fn current_slot(&self, flash: &mut FlashStorage) -> Option<(u8, u32, Option<OtaMetadata>)> {
        let mut best_partition = None;
        let mut best_seq = 0;
        let mut best_metadata = None;
        for slot in 0u8..=1 {
            let mut bytes = [0u8; (ESP32_OTADATA_SIZE + FS_PARTITION_METADATA_SIZE) as usize];
            flash
                .read(self.otadata_offset + slot as u32 * SECTOR_SIZE, &mut bytes)
                .unwrap();
            let seq = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            let implied_cs = CRC.checksum(&seq.to_le_bytes());
            let actual_cs = u32::from_le_bytes(bytes[28..28 + 4].try_into().unwrap());
            let metadata = bincode::decode_from_slice::<OtaMetadata, _>(
                &bytes[ESP32_OTADATA_SIZE as usize..],
                bincode::config::standard(),
            )
            .ok()
            .map(|(metadata, _)| metadata);
            if implied_cs != actual_cs {
                continue;
            }

            if best_partition.is_none() || seq >= best_seq {
                best_partition = Some(slot);
                best_seq = seq;
                best_metadata = metadata;
            }
        }

        let current_partition = best_partition?;
        Some((current_partition, best_seq, best_metadata))
    }

    pub fn read_metadata_in_slot(&self, flash: &mut FlashStorage, slot: u8) -> Option<OtaMetadata> {
        let mut bytes = [0u8; FS_PARTITION_METADATA_SIZE as usize];
        flash
            .read(
                self.otadata_offset + (slot as u32) * SECTOR_SIZE + ESP32_OTADATA_SIZE,
                &mut bytes[..],
            )
            .unwrap();
        if bytes != [0xff; FS_PARTITION_METADATA_SIZE as usize] {
            let (metadata, _) =
                bincode::decode_from_slice(&bytes[..], bincode::config::standard()).ok()?;
            Some(metadata)
        } else {
            None
        }
    }

    /// Write to the otadata parition to indicate that a different partition should be the main one.
    fn switch_partition(&self, flash: &mut FlashStorage, slot: u8, metadata: Option<OtaMetadata>) {
        // to select it the parition must be higher than the other one
        let next_seq = match self.current_slot(flash) {
            Some((current_slot, seq, _)) => {
                if slot == current_slot {
                    /* do nothing */
                    return;
                } else {
                    seq + 1
                }
            }
            None => 1,
        };

        // it also needs a valid checksum on the parition
        let cs = CRC.checksum(&next_seq.to_le_bytes());
        let mut bytes = [0xffu8; ESP32_OTADATA_SIZE as usize + FS_PARTITION_METADATA_SIZE as usize];
        bytes[0..4].copy_from_slice(&next_seq.to_le_bytes());
        bytes[28..28 + 4].copy_from_slice(&cs.to_le_bytes());

        if let Some(metadata) = metadata {
            bincode::encode_into_slice(metadata, &mut bytes[32..], bincode::config::standard())
                .expect("ota metadata could not be encoded");
        }

        flash
            .write(self.otadata_offset + (slot as u32) * 4096, &bytes)
            .unwrap();
        let mut read = [0u8; ESP32_OTADATA_SIZE as usize + FS_PARTITION_METADATA_SIZE as usize];
        flash
            .read(self.otadata_offset + (slot as u32) * 4096, &mut read)
            .unwrap();

        assert_eq!(read, bytes, "otadata read should be what was written");
    }

    pub fn start_upgrade(
        &self,
        flash: &mut FlashStorage,
        size: u32,
        expected_digest: FirmwareDigest,
        active_partition_digest: FirmwareDigest,
    ) -> FirmwareUpgradeMode {
        let slot = self.next_slot(flash);
        let partition = self.ota_partitions[slot as usize];
        assert_eq!(
            partition.size, FIRMWARE_IMAGE_SIZE,
            "partition size and FIRMWARE_PAD_LENGTH must be the same"
        );
        assert!(
            partition.size % FIRMWARE_UPGRADE_CHUNK_LEN == 0,
            "these should match up to avoid overwriting"
        );

        if expected_digest == active_partition_digest {
            FirmwareUpgradeMode::Passive {
                size,
                sent_ack: false,
            }
        } else {
            FirmwareUpgradeMode::Upgrading {
                ota: *self,
                ota_slot: slot,
                expected_digest,
                size,
                state: State::WaitingForConfirm { sent_prompt: false },
            }
        }
    }
}

#[derive(Debug)]
pub enum FirmwareUpgradeMode {
    Upgrading {
        ota: OtaConfig,
        ota_slot: u8,
        expected_digest: FirmwareDigest,
        size: u32,
        state: State,
    },
    Passive {
        size: u32,
        sent_ack: bool,
    },
}

#[derive(Clone, Debug)]
pub enum State {
    WaitingForConfirm { sent_prompt: bool },
    Erase { seq: u32 },
    WaitingToEnterUpgradeMode,
}

impl FirmwareUpgradeMode {
    pub fn poll(
        &mut self,
        flash: &mut FlashStorage,
    ) -> (Option<DeviceSendBody>, Option<ui::Workflow>) {
        match self {
            FirmwareUpgradeMode::Upgrading {
                ota,
                ota_slot,
                expected_digest,
                size,
                state,
            } => {
                let partition = ota.ota_partitions[*ota_slot as usize];
                match state {
                    State::WaitingForConfirm { sent_prompt } if !*sent_prompt => {
                        *sent_prompt = true;
                        (
                            None,
                            Some(ui::Workflow::prompt(ui::Prompt::ConfirmFirmwareUpgrade {
                                firmware_digest: *expected_digest,
                                size: *size,
                            })),
                        )
                    }
                    State::Erase { seq } => {
                        let mut finished = false;
                        /// So we erase multiple sectors poll (otherwise it's slow).
                        const ERASE_CHUNK_SIZE: usize = 32;
                        for _ in 0..ERASE_CHUNK_SIZE {
                            // it's faster to read and check if it's already erased than just to go
                            // and erase it
                            if partition
                                .get_sector(flash, *seq)
                                .unwrap()
                                .iter()
                                .any(|byte| *byte != 0xff)
                            {
                                partition.erase_image_sector(flash, *seq);
                            }
                            *seq += 1;
                            if *seq == SECTORS_PER_IMAGE {
                                finished = true;
                                break;
                            }
                        }

                        let status = Some(ui::FirmwareUpgradeStatus::Erase {
                            progress: *seq as f32 / SECTORS_PER_IMAGE as f32,
                        });

                        let status = status.map(|status| {
                            ui::Workflow::BusyDoing(ui::BusyTask::FirmwareUpgrade(status))
                        });

                        if finished {
                            *state = State::WaitingToEnterUpgradeMode;
                            (Some(DeviceSendBody::AckUpgradeMode), status)
                        } else {
                            (None, status)
                        }
                    }
                    _ => {
                        /* waiting */
                        (None, None)
                    }
                }
            }
            FirmwareUpgradeMode::Passive { sent_ack, .. } => {
                if !*sent_ack {
                    *sent_ack = true;
                    (
                        Some(DeviceSendBody::AckUpgradeMode),
                        Some(ui::Workflow::BusyDoing(ui::BusyTask::FirmwareUpgrade(
                            ui::FirmwareUpgradeStatus::Passive,
                        ))),
                    )
                } else {
                    (None, None)
                }
                /* we will passively forward data for upgrade no need to prompt or do anything */
            }
        }
    }

    pub fn upgrade_confirm(&mut self) {
        match self {
            FirmwareUpgradeMode::Upgrading {
                state: state @ State::WaitingForConfirm { sent_prompt: true },
                ..
            } => {
                *state = State::Erase { seq: 0 };
            }
            _ => {
                panic!(
                    "Upgrade confirmed while not waiting for a confirmation. {:?}",
                    self
                );
            }
        }
    }

    pub fn enter_upgrade_mode(
        &mut self,
        flash: &mut FlashStorage,
        upstream_io: &mut SerialIo<'_, impl uart::Instance>,
        mut downstream_io: Option<&mut SerialIo<'_, impl uart::Instance>>,
        ui: &mut impl UserInteraction,
        sha: &mut Sha<'_, Blocking>,
    ) {
        match self {
            FirmwareUpgradeMode::Upgrading { state, .. } => {
                if !matches!(state, State::WaitingToEnterUpgradeMode) {
                    panic!("can't start upgrade while still preparing");
                }
            }
            FirmwareUpgradeMode::Passive { .. } => { /* always ready to enter upgrade mode */ }
        }

        let upgrade_size = match *self {
            FirmwareUpgradeMode::Upgrading { size, .. } => size,
            FirmwareUpgradeMode::Passive { size, .. } => size,
        };

        upstream_io.change_baud(OTA_UPDATE_BAUD);
        if let Some(downstream_io) = downstream_io.as_mut() {
            downstream_io.change_baud(OTA_UPDATE_BAUD);
        }

        // allocate it on heap with Box to avoid enlarging stack
        let mut in_buf = Box::new([0xffu8; SECTOR_SIZE as usize]);
        let mut i = 0;
        let mut byte_count = 0;
        let mut sector = 0;

        let mut finished_writing = false;
        let mut downstream_ready = downstream_io.is_none();
        let mut told_upstream_im_ready = false;

        while !finished_writing {
            if downstream_ready {
                if let Ok(byte) = upstream_io.read_byte() {
                    in_buf[i] = byte;
                    i += 1;
                    byte_count += 1;
                    finished_writing = byte_count == upgrade_size;
                    if let Some(downstream_io) = downstream_io.as_mut() {
                        block!(downstream_io.write_byte_nb(byte)).unwrap();
                    }

                    if i == SECTOR_SIZE as _ || finished_writing {
                        // we know the downstream device (if it exists) might be writing to flash so
                        // assume it's not ready yet.
                        downstream_ready = downstream_io.is_none();
                        // likewise the upstream device assumes we're not ready
                        told_upstream_im_ready = false;
                        i = 0;
                        // only write to the partition if we're actually upgrading
                        if let FirmwareUpgradeMode::Upgrading {
                            ota_slot,
                            ota,
                            size,
                            ..
                        } = &self
                        {
                            let partition = ota.ota_partitions[*ota_slot as usize];
                            partition.write_sector(flash, sector, &in_buf).unwrap();
                            ui.set_workflow(ui::Workflow::BusyDoing(
                                ui::BusyTask::FirmwareUpgrade(
                                    ui::FirmwareUpgradeStatus::Download {
                                        progress: byte_count as f32 / *size as f32,
                                    },
                                ),
                            ));
                            ui.poll();
                        }
                        in_buf.fill(0xff);
                        sector += 1;
                    }
                }
            }

            if !finished_writing {
                if let Some(downstream_io) = downstream_io.as_mut() {
                    while let Ok(byte) = downstream_io.read_byte() {
                        assert!(
                            byte == FIRMWARE_NEXT_CHUNK_READY_SIGNAL,
                            "invalid control byte sent by downstream"
                        );
                        downstream_ready = true;
                    }
                }

                if downstream_ready && !told_upstream_im_ready {
                    block!(upstream_io.write_byte_nb(FIRMWARE_NEXT_CHUNK_READY_SIGNAL)).unwrap();
                    upstream_io.nb_flush();
                    told_upstream_im_ready = true;
                }
            }
        }

        ui.poll();

        if let Some(downstream_io) = downstream_io.as_mut() {
            downstream_io.flush();
        }

        // change it back to the original baudrate but keep in mind that the devices are meant to
        // restart after the upgrade.
        upstream_io.change_baud(BAUDRATE);
        if let Some(downstream_io) = downstream_io.as_mut() {
            downstream_io.change_baud(BAUDRATE);
        }

        if let FirmwareUpgradeMode::Upgrading {
            ota_slot,
            expected_digest,
            ota,
            ..
        } = &self
        {
            let partition = ota.ota_partitions[*ota_slot as usize];
            let digest = partition.digest(flash, sha);
            if digest == *expected_digest {
                let metadata = OtaMetadata {};
                ota.switch_partition(flash, *ota_slot, Some(metadata));
            } else {
                panic!(
                    "upgrade downloaded did not match intended digest. \nGot:\n{digest}\nExpected:\n{}",
                    expected_digest
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, bincode::Encode, bincode::Decode)]
pub struct OtaMetadata {/* it's empty for now but this is where I would put signatures on the firmware etc */}
