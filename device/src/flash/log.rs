use alloc::{boxed::Box, string::String};
use embedded_storage::nor_flash::NorFlash;
use frostsnap_core::{
    device::{self, SaveShareMutation},
    schnorr_fun::frost::SecretShare,
};
use frostsnap_embedded::{AbSlot, FlashPartition, NorFlashLog};

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum Event {
    Core(device::Mutation),
    Name(String),
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, Default)]
pub struct ShareSlot {
    pub saved_share: Option<SaveShareMutation>,
    pub recovering_share: Option<SecretShare>,
}

pub struct EventLog<'a, S> {
    log: NorFlashLog<'a, S>,
    share_slot: AbSlot<'a, S>,
}

impl<'a, S: NorFlash> EventLog<'a, S> {
    pub fn new(
        mut share_flash: FlashPartition<'a, S>,
        mut log_flash: FlashPartition<'a, S>,
    ) -> Self {
        log_flash.tag = "event-log";
        share_flash.tag = "share";
        let share_slot = AbSlot::new(share_flash);
        EventLog {
            log: NorFlashLog::new(log_flash),
            share_slot,
        }
    }

    pub fn push(&mut self, value: Event) -> Result<(), bincode::error::EncodeError> {
        match &value {
            Event::Core(device::Mutation::SaveShare(save_share_mutation)) => {
                self.share_slot.write(&ShareSlot {
                    saved_share: Some(*save_share_mutation.clone()),
                    ..Default::default()
                });
            }
            _ => {
                self.log.push(value)?;
            }
        }
        Ok(())
    }

    pub fn seek_iter(
        &mut self,
    ) -> impl Iterator<Item = Result<Event, bincode::error::DecodeError>> + use<'_, 'a, S> {
        self.log
            .seek_iter::<Event>()
            .chain(self.share_slot.read::<ShareSlot>().and_then(|share_slot| {
                Some(Ok(Event::Core(device::Mutation::SaveShare(Box::new(
                    share_slot.saved_share?,
                )))))
            }))
    }

    pub fn append(
        &mut self,
        iter: impl IntoIterator<Item = Event>,
    ) -> Result<(), bincode::error::EncodeError> {
        for item in iter {
            self.push(item)?;
        }
        Ok(())
    }

    pub fn read_share_slot(&self) -> Option<ShareSlot> {
        self.share_slot.read()
    }
}
