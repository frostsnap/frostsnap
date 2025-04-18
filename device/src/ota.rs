use crate::{
    io::SerialIo,
    partitions::{EspFlashPartition, PartitionExt},
    ui::{self, UserInteraction},
};
use alloc::boxed::Box;
use bincode::config::{Fixint, LittleEndian};
use esp_hal::sha::Sha;
use frostsnap_comms::{
    CommsMisc, DeviceSendBody, Sha256Digest, BAUDRATE, FIRMWARE_IMAGE_SIZE,
    FIRMWARE_NEXT_CHUNK_READY_SIGNAL, FIRMWARE_UPGRADE_CHUNK_LEN,
};
use nb::block;

#[derive(Clone, Debug)]
pub struct OtaPartitions<'a> {
    pub otadata: EspFlashPartition<'a>,
    pub ota_0: EspFlashPartition<'a>,
    pub ota_1: EspFlashPartition<'a>,
    pub factory: EspFlashPartition<'a>,
}

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
const SECTORS_PER_IMAGE: u32 = FIRMWARE_IMAGE_SIZE / SECTOR_SIZE;
/// We switch the baudrate during OTA update to make it faster
const OTA_UPDATE_BAUD: u32 = 921_600;

/// we want fixint encoding for the otadata section because that's what esp32 uses.
const OTADATA_BINCODE_CONFIG: bincode::config::Configuration<LittleEndian, Fixint> =
    bincode::config::legacy();

/// This is the somewhat undocumented layout of each of the two otadata slots.
/// The seq_crc is just a crc on the seq value.
#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq)]
struct EspOtadataSlot {
    seq: u32,
    reserved: [u8; 24],
    seq_crc: u32,
    /// defined by us
    our_metadata: OtaMetadata,
}

impl<'a> OtaPartitions<'a> {
    fn next_slot(&self) -> usize {
        self.current_slot().map(|(i, _)| (i + 1) % 2).unwrap_or(0)
    }

    fn ota_partitions(&self) -> [EspFlashPartition<'a>; 2] {
        [self.ota_0, self.ota_1]
    }

    fn otadata_sectors(&self) -> [EspFlashPartition<'a>; 2] {
        let mut ota_0_desc = self.otadata;
        let ota_1_desc = ota_0_desc.split_off_end(1);
        [ota_0_desc, ota_1_desc]
    }

    pub fn active_partition(&self) -> EspFlashPartition<'a> {
        match self.current_slot() {
            Some((slot, _)) => self.ota_partitions()[slot],
            None => self.factory,
        }
    }

    fn current_slot(&self) -> Option<(usize, EspOtadataSlot)> {
        let mut best_partition: Option<(usize, EspOtadataSlot)> = None;
        for (i, slot) in self.otadata_sectors().into_iter().enumerate() {
            let otadata_slot = bincode::decode_from_reader::<EspOtadataSlot, _, _>(
                slot.bincode_reader(),
                OTADATA_BINCODE_CONFIG,
            );

            let otadata_slot = match otadata_slot {
                Ok(otadata_slot) => otadata_slot,
                Err(_) => continue,
            };
            let implied_crc = CRC.checksum(&otadata_slot.seq.to_le_bytes());
            if implied_crc != otadata_slot.seq_crc {
                continue;
            }

            if best_partition.as_ref().map(|(_, data)| data.seq) < Some(otadata_slot.seq) {
                best_partition = Some((i, otadata_slot));
            }
        }

        best_partition
    }

    /// Write to the otadata parition to indicate that a different partition should be the main one.
    fn switch_partition(&self, slot: usize, metadata: OtaMetadata) {
        // to select it the parition must be higher than the other one
        let next_seq = match self.current_slot() {
            Some((current_slot, otadata_slot)) => {
                if slot == current_slot {
                    /* do nothing */
                    return;
                } else {
                    otadata_slot
                        .seq
                        .checked_add(1)
                        .expect("practically unreachable")
                }
            }
            None => 1,
        };

        // it also needs a valid checksum on the parition
        let seq_crc = CRC.checksum(&next_seq.to_le_bytes());
        let otadata = EspOtadataSlot {
            seq: next_seq,
            reserved: Default::default(),
            seq_crc,
            our_metadata: metadata,
        };

        let target = self.otadata_sectors()[slot];
        target.erase_all().expect("failed to erase");
        let mut writer = target.bincode_writer_remember_to_flush::<64>();
        bincode::encode_into_writer(&otadata, &mut writer, OTADATA_BINCODE_CONFIG)
            .expect("failed to write otadata");
        let _ = writer.flush().expect("failed to switch parition");
        let what_was_written: EspOtadataSlot =
            bincode::decode_from_reader(target.bincode_reader(), OTADATA_BINCODE_CONFIG)
                .expect("failed to read back what was written");

        assert_eq!(
            what_was_written, otadata,
            "check that what was written was right"
        );
    }

    pub fn start_upgrade(
        &self,
        size: u32,
        expected_digest: Sha256Digest,
        active_partition_digest: Sha256Digest,
    ) -> FirmwareUpgradeMode {
        let slot = self.next_slot();
        let partition = &self.ota_partitions()[slot];
        assert_eq!(
            partition.size(),
            FIRMWARE_IMAGE_SIZE,
            "partition size should be the same as FIRMWARE_IMAGE_SIZE"
        );
        assert!(
            partition.size() % FIRMWARE_UPGRADE_CHUNK_LEN == 0,
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
pub enum FirmwareUpgradeMode<'a> {
    Upgrading {
        ota: OtaPartitions<'a>,
        ota_slot: usize,
        expected_digest: Sha256Digest,
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

impl FirmwareUpgradeMode<'_> {
    pub fn poll(&mut self, ui: &mut impl crate::ui::UserInteraction) -> Option<DeviceSendBody> {
        match self {
            FirmwareUpgradeMode::Upgrading {
                ota,
                ota_slot,
                expected_digest,
                size,
                state,
            } => {
                let partition = ota.ota_partitions()[*ota_slot];
                match state {
                    State::WaitingForConfirm { sent_prompt } if !*sent_prompt => {
                        *sent_prompt = true;
                        ui.set_workflow(ui::Workflow::prompt(ui::Prompt::ConfirmFirmwareUpgrade {
                            firmware_digest: *expected_digest,
                            size: *size,
                        }));
                        None
                    }
                    State::Erase { seq } => {
                        let mut finished = false;
                        /// So we erase multiple sectors poll (otherwise it's slow).
                        const ERASE_CHUNK_SIZE: usize = 32;
                        for _ in 0..ERASE_CHUNK_SIZE {
                            // it's faster to read and check if it's already erased than just to go
                            // and erase it
                            if partition
                                .read_sector(*seq)
                                .unwrap()
                                .iter()
                                .any(|byte| *byte != 0xff)
                            {
                                partition.erase_sector(*seq).expect("must erase sector");
                            }
                            *seq += 1;
                            if *seq == SECTORS_PER_IMAGE {
                                finished = true;
                                break;
                            }
                        }

                        ui.set_workflow(ui::Workflow::FirmwareUpgrade(
                            ui::FirmwareUpgradeStatus::Erase {
                                progress: *seq as f32 / SECTORS_PER_IMAGE as f32,
                            },
                        ));

                        if finished {
                            *state = State::WaitingToEnterUpgradeMode;
                            Some(DeviceSendBody::Misc(CommsMisc::AckUpgradeMode))
                        } else {
                            None
                        }
                    }
                    _ => {
                        /* waiting */
                        None
                    }
                }
            }
            FirmwareUpgradeMode::Passive { sent_ack, .. } => {
                if !*sent_ack {
                    *sent_ack = true;
                    ui.set_workflow(ui::Workflow::FirmwareUpgrade(
                        ui::FirmwareUpgradeStatus::Passive,
                    ));
                    Some(DeviceSendBody::Misc(CommsMisc::AckUpgradeMode))
                } else {
                    None
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
                            let partition = ota.ota_partitions()[*ota_slot];
                            partition.nor_write_sector(sector, &in_buf).unwrap();
                            ui.set_workflow(ui::Workflow::FirmwareUpgrade(
                                ui::FirmwareUpgradeStatus::Download {
                                    progress: byte_count as f32 / *size as f32,
                                },
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
            let partition = &ota.ota_partitions()[*ota_slot];
            let digest = partition.sha256_digest(sha);
            if digest == *expected_digest {
                ota.switch_partition(*ota_slot, OtaMetadata::default());
            } else {
                panic!(
                    "upgrade downloaded did not match intended digest. \nGot:\n{digest}\nExpected:\n{}",
                    expected_digest
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, bincode::Encode, bincode::Decode, PartialEq)]
pub struct OtaMetadata {/* it's empty for now but this is where I would put signatures on the firmware etc */}
