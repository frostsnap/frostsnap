use crate::FlashPartition;
use alloc::boxed::Box;
use embedded_storage::nor_flash::NorFlash;
use frostsnap_comms::{CommsMisc, DeviceSendBody};

const CHUNK_SIZE: u32 = 16; // 64KB chunks

pub struct Erase {
    state: State,
    n_sectors: u32,
    header_sectors: u32,
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
    /// Report zero progress, transition to ErasingFirst
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
    /// `header_sectors` is the number of sectors at the front of `partition` holding the header.
    pub fn new<S: NorFlash>(partition: &FlashPartition<S>, header_sectors: u32) -> Self {
        Self {
            state: State::Initial,
            n_sectors: partition.n_sectors(),
            header_sectors,
        }
    }

    /// `on_progress` is called with the number of sectors erased so far and the total.
    pub fn poll<S: NorFlash>(
        &mut self,
        partition: &FlashPartition<S>,
        mut on_progress: impl FnMut(u32, u32),
    ) -> ErasePoll {
        match self.state {
            State::Initial => {
                on_progress(0, self.n_sectors);
                self.state = State::ErasingFirst;
                ErasePoll::Pending
            }
            State::ErasingFirst => {
                partition
                    .erase_sectors(0, self.header_sectors)
                    .expect("failed to erase header sectors");
                on_progress(self.header_sectors, self.n_sectors);
                self.state = State::ErasingRest;
                ErasePoll::SendConfirmation(Box::new(DeviceSendBody::Misc(
                    CommsMisc::EraseConfirmed,
                )))
            }
            State::ErasingRest => {
                let mut sector = self.header_sectors;
                while sector < self.n_sectors {
                    let count = CHUNK_SIZE.min(self.n_sectors - sector);
                    partition
                        .erase_sectors(sector, count)
                        .expect("failed to erase sectors");
                    sector += count;
                    on_progress(sector, self.n_sectors);
                }
                self.state = State::Done;
                ErasePoll::Reset
            }
            State::Done => ErasePoll::Reset,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{test::TestNorFlash, AbSlot};
    use core::cell::RefCell;

    const HEADER_SECTORS: u32 = 2;

    /// The coordinator drops the device from the wallet as soon as it sees `EraseConfirmed`, so
    /// by then the header must be unreadable from *both* A/B slots. If either survived, a power
    /// cut after confirming would reboot the device with its old identity and shares.
    #[test]
    fn header_is_fully_erased_before_confirmation() {
        let flash = RefCell::new(TestNorFlash::new());
        let nvs = FlashPartition::new(&flash, 0, 4, "nvs");
        let mut rest = nvs;
        let header = AbSlot::new(rest.split_off_front(HEADER_SECTORS));
        header.write(&0xdead_beef_u32);
        AbSlot::new(rest).write(&7u32);
        assert_eq!(header.read::<u32>(), Some(0xdead_beef));

        let mut erase = Erase::new(&nvs, HEADER_SECTORS);
        loop {
            match erase.poll(&nvs, |_, _| {}) {
                ErasePoll::Pending => {}
                ErasePoll::SendConfirmation(msg) => {
                    assert!(matches!(
                        *msg,
                        DeviceSendBody::Misc(CommsMisc::EraseConfirmed)
                    ));
                    break;
                }
                ErasePoll::Reset => panic!("reset before sending confirmation"),
            }
        }

        for sector in 0..HEADER_SECTORS {
            assert!(
                nvs.read_sector(sector).unwrap().iter().all(|b| *b == 0xff),
                "header sector {sector} not erased before confirmation"
            );
        }
        assert_eq!(header.read::<u32>(), None);

        while !matches!(erase.poll(&nvs, |_, _| {}), ErasePoll::Reset) {}
        assert!(nvs.is_empty().unwrap());
    }
}
