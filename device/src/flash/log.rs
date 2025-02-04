use alloc::string::String;
use embedded_storage::nor_flash::NorFlash;
use frostsnap_core::device;
use frostsnap_embedded::{FlashPartition, NorFlashLog};

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum Event {
    Core(device::Mutation),
    Name(String),
}

pub struct EventLog<'a, S> {
    log: NorFlashLog<'a, S>,
}

impl<'a, S: NorFlash> EventLog<'a, S> {
    pub fn new(mut flash: FlashPartition<'a, S>) -> Self {
        flash.tag = "event-log";
        EventLog {
            log: NorFlashLog::new(flash),
        }
    }

    pub fn push(&mut self, value: Event) -> Result<(), bincode::error::EncodeError> {
        self.log.push(value)
    }

    pub fn seek_iter(
        &mut self,
    ) -> impl Iterator<Item = Result<Event, bincode::error::DecodeError>> + use<'_, 'a, S> {
        self.log.seek_iter::<Event>()
    }

    pub fn append(
        &mut self,
        iter: impl IntoIterator<Item = Event>,
    ) -> Result<(), bincode::error::EncodeError> {
        for item in iter {
            self.log.push(item)?;
        }
        Ok(())
    }
}
