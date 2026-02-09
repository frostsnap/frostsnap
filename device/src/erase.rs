use crate::ui::{self, UserInteraction};
use alloc::boxed::Box;
use frostsnap_comms::{CommsMisc, DeviceSendBody};
use frostsnap_embedded::FlashPartition;
use frostsnap_widgets::Frac;

const CHUNK_SIZE: u32 = 16; // 64KB chunks

pub struct Erase {
    state: State,
    n_sectors: u32,
}

/// Erase state machine. We split the erase into two phases so we can send confirmation after the
/// first sector is erased. This proves to the coordinator that data destruction has begun and it's
/// safe to remove the device from access structures, even if the device loses power before
/// completing the full erase.
enum State {
    /// Show progress bar, transition to ErasingFirst
    Initial,
    /// Erase first sector, send confirmation, transition to ErasingRest
    ErasingFirst,
    /// Erase remaining sectors in a blocking loop with progress updates, then reset
    ErasingRest,
    Done,
}

pub enum ErasePoll {
    Pending,
    SendConfirmation(Box<DeviceSendBody>),
    Reset,
}

impl Erase {
    pub fn new<S>(partition: &FlashPartition<S>) -> Self
    where
        S: embedded_storage::nor_flash::NorFlash,
    {
        Self {
            state: State::Initial,
            n_sectors: partition.n_sectors(),
        }
    }

    pub fn poll<S>(
        &mut self,
        partition: &FlashPartition<S>,
        ui: &mut impl UserInteraction,
    ) -> ErasePoll
    where
        S: embedded_storage::nor_flash::NorFlash,
    {
        match self.state {
            State::Initial => {
                ui.set_workflow(ui::Workflow::EraseProgress {
                    progress: Frac::ZERO,
                });
                self.state = State::ErasingFirst;
                ErasePoll::Pending
            }
            State::ErasingFirst => {
                partition.erase_sector(0).expect("failed to erase sector");
                ui.set_workflow(ui::Workflow::EraseProgress {
                    progress: Frac::from_ratio(1, self.n_sectors),
                });
                self.state = State::ErasingRest;
                ErasePoll::SendConfirmation(Box::new(DeviceSendBody::Misc(
                    CommsMisc::EraseConfirmed,
                )))
            }
            State::ErasingRest => {
                let mut sector = 1u32;
                while sector < self.n_sectors {
                    let count = CHUNK_SIZE.min(self.n_sectors - sector);
                    partition
                        .erase_sectors(sector, count)
                        .expect("failed to erase sectors");
                    sector += count;

                    ui.set_workflow(ui::Workflow::EraseProgress {
                        progress: Frac::from_ratio(sector, self.n_sectors),
                    });
                    ui.poll();
                }
                self.state = State::Done;
                ErasePoll::Reset
            }
            State::Done => ErasePoll::Reset,
        }
    }
}
