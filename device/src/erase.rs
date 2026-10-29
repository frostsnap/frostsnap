use crate::flash::HEADER_SECTORS;
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
/// header is erased. This proves to the coordinator that data destruction has begun and it's
/// safe to remove the device from access structures, even if the device loses power before
/// completing the full erase.
///
/// The header is an A/B slot, so *both* of its sectors must go before confirming: with only one
/// erased, the other still reads as a valid header and the device would boot with its old
/// identity and shares intact instead of resuming the erase.
enum State {
    /// Show progress bar, transition to ErasingFirst
    Initial,
    /// Erase the header sectors, send confirmation, transition to ErasingRest
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
                partition
                    .erase_sectors(0, HEADER_SECTORS)
                    .expect("failed to erase header sectors");
                ui.set_workflow(ui::Workflow::EraseProgress {
                    progress: Frac::from_ratio(HEADER_SECTORS, self.n_sectors),
                });
                self.state = State::ErasingRest;
                ErasePoll::SendConfirmation(Box::new(DeviceSendBody::Misc(
                    CommsMisc::EraseConfirmed,
                )))
            }
            State::ErasingRest => {
                let mut sector = HEADER_SECTORS;
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
