use crate::{
    io::SerialIo,
    ui::{self, UserInteraction},
};
use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;
use embedded_storage::{nor_flash, ReadStorage, Storage};
use esp_hal::sha::{Sha, Sha256};
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

#[derive(Debug, Clone)]
pub struct OtaFlash {
    flash: Rc<RefCell<FlashStorage>>,
    otadata_offset: u32,
    factory_partition: Partition,
    ota_partitions: [Partition; 2],
}

#[derive(Debug, Clone, Default)]
pub struct Partition {
    offset: u32,
    size: u32,
    flash: Rc<RefCell<FlashStorage>>,
}

impl Partition {
    pub fn erase_image_sector(&self, sector: u32) {
        if sector >= SECTORS_PER_IMAGE {
            panic!("tried to erase sector out of bounds");
        }
        let start = self.offset + sector * SECTOR_SIZE;
        nor_flash::NorFlash::erase(&mut *self.flash.borrow_mut(), start, start + SECTOR_SIZE)
            .unwrap();
    }

    pub fn digest(&self, sha256: &mut Sha<'_>) -> FirmwareDigest {
        let mut hasher = sha256.start::<Sha256>();
        for i in 0..(self.size / SECTOR_SIZE) {
            let sector = self.get_sector(i).unwrap();
            let mut remaining = &sector[..];
            while !remaining.is_empty() {
                remaining = block!(hasher.update(remaining)).unwrap();
            }
        }
        let mut digest = FirmwareDigest([0u8; 32]);
        block!(hasher.finish(&mut digest.0)).unwrap();
        digest
    }

    pub fn get_sector(&self, sector: u32) -> Result<[u8; SECTOR_SIZE as usize], &'static str> {
        if sector >= SECTORS_PER_IMAGE {
            return Err("requested sector is out of bounds");
        }
        let mut ret = [0u8; SECTOR_SIZE as usize];

        self.flash
            .borrow_mut()
            .read(self.offset + sector * SECTOR_SIZE, &mut ret)
            .unwrap();
        Ok(ret)
    }

    pub fn write_sector(
        &self,
        sector: u32,
        bytes: &[u8; SECTOR_SIZE as usize],
    ) -> Result<(), &'static str> {
        if sector >= SECTORS_PER_IMAGE {
            return Err("can't write sector out of bounds");
        }
        nor_flash::NorFlash::write(
            &mut *self.flash.borrow_mut(),
            self.offset + sector * SECTOR_SIZE,
            &bytes[..],
        )
        .expect("hardware specific error");
        Ok(())
    }
}

impl OtaFlash {
    pub fn new(flash: Rc<RefCell<FlashStorage>>) -> Self {
        let table = esp_partition_table::PartitionTable::new(0x8000, 10 * 32);
        let mut otadata_offset = Default::default();
        let mut ota_partitions: [_; 2] = Default::default();
        let mut factory_partition = Default::default();

        for row in table.iter_storage(&mut *flash.borrow_mut(), false) {
            let row = match row {
                Ok(row) => row,
                Err(_) => panic!("unable to read row of partition table"),
            };
            match row.name() {
                "otadata" => {
                    otadata_offset = Some(row.offset);
                }
                "ota_0" => {
                    ota_partitions[0] = Some((row.offset, row.size));
                }
                "ota_1" => {
                    ota_partitions[1] = Some((row.offset, row.size));
                }
                "factory" => {
                    factory_partition = Some((row.offset, row.size));
                }
                _ => { /*ignore*/ }
            }
        }

        let factory_partition = factory_partition.unwrap();

        Self {
            otadata_offset: otadata_offset.unwrap(),
            ota_partitions: [
                Partition {
                    offset: ota_partitions[0].unwrap().0,
                    size: ota_partitions[0].unwrap().1 as u32,
                    flash: flash.clone(),
                },
                Partition {
                    offset: ota_partitions[1].unwrap().0,
                    size: ota_partitions[1].unwrap().1 as u32,
                    flash: flash.clone(),
                },
            ],
            factory_partition: Partition {
                offset: factory_partition.0,
                size: factory_partition.1 as u32,
                flash: flash.clone(),
            },
            flash: flash.clone(),
        }
    }

    fn next_slot(&self) -> u8 {
        self.current_slot()
            .map(|(i, _, _)| (i + 1) % 2)
            .unwrap_or(0)
    }

    pub fn active_partition(&self) -> Partition {
        match self.current_slot() {
            Some((slot, _, _)) => self.ota_partitions[slot as usize].clone(),
            None => self.factory_partition.clone(),
        }
    }

    pub fn current_slot(&self) -> Option<(u8, u32, Option<OtaMetadata>)> {
        let mut best_partition = None;
        let mut best_seq = 0;
        let mut best_metadata = None;
        for slot in 0u8..=1 {
            let mut bytes = [0u8; (ESP32_OTADATA_SIZE + FS_PARTITION_METADATA_SIZE) as usize];
            self.flash
                .borrow_mut()
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

    pub fn read_metadata_in_slot(&self, slot: u8) -> Option<OtaMetadata> {
        let mut bytes = [0u8; FS_PARTITION_METADATA_SIZE as usize];
        self.flash
            .borrow_mut()
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
    fn switch_partition(&self, slot: u8, metadata: Option<OtaMetadata>) {
        // to select it the parition must be higher than the other one
        let next_seq = match self.current_slot() {
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

        self.flash
            .borrow_mut()
            .write(self.otadata_offset + (slot as u32) * 4096, &bytes)
            .unwrap();
        let mut read = [0u8; ESP32_OTADATA_SIZE as usize + FS_PARTITION_METADATA_SIZE as usize];
        self.flash
            .borrow_mut()
            .read(self.otadata_offset + (slot as u32) * 4096, &mut read)
            .unwrap();

        assert_eq!(read, bytes, "otadata read should be what was written");
    }

    pub fn start_upgrade(
        &self,
        size: u32,
        expected_digest: FirmwareDigest,
        active_partition_digest: FirmwareDigest,
    ) -> FirmwareUpgradeMode {
        let slot = self.next_slot();
        let partition = &self.ota_partitions[slot as usize];
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
                ota: self.clone(),
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
        ota: OtaFlash,
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
    pub fn poll(&mut self) -> (Option<DeviceSendBody>, Option<ui::Workflow>) {
        match self {
            FirmwareUpgradeMode::Upgrading {
                ota,
                ota_slot,
                expected_digest,
                size,
                state,
            } => {
                let partition = ota.ota_partitions[*ota_slot as usize].clone();
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
                                .get_sector(*seq)
                                .unwrap()
                                .iter()
                                .any(|byte| *byte != 0xff)
                            {
                                partition.erase_image_sector(*seq);
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
        upstream_io: &mut SerialIo<'_>,
        mut downstream_io: Option<&mut SerialIo<'_>>,
        ui: &mut impl UserInteraction,
        sha: &mut Sha<'_>,
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
                            let partition = ota.ota_partitions[*ota_slot as usize].clone();
                            partition.write_sector(sector, &in_buf).unwrap();
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
            let partition = &ota.ota_partitions[*ota_slot as usize];
            let digest = partition.digest(sha);
            if digest == *expected_digest {
                let metadata = OtaMetadata {};
                ota.switch_partition(*ota_slot, Some(metadata));
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
