use alloc::{boxed::Box, string::String};
use embedded_storage::nor_flash::NorFlash;
use frostsnap_core::device::{self, SaveShareMutation};
use frostsnap_embedded::{AbSlot, FlashPartition, NorFlashLog};

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum Mutation {
    Core(device::Mutation),
    Name(String),
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum ShareSlot {
    SecretShare(Box<SaveShareMutation>),
    SavedBackup(device::restoration::SavedBackup),
}

impl ShareSlot {
    pub fn into_mutations(self) -> impl IntoIterator<Item = device::Mutation> {
        core::iter::once(match self {
            ShareSlot::SecretShare(save_share_mutation) => {
                device::Mutation::Keygen(device::keys::KeyMutation::SaveShare(save_share_mutation))
            }
            ShareSlot::SavedBackup(saved_backup) => device::Mutation::Restoration(
                device::restoration::RestorationMutation::Save(saved_backup),
            ),
        })
    }
}

/// Mutation log saves the device's core mutations but treats secret share saving mutations
/// differently to enforce the one secret share per device rule.
pub struct MutationLog<'a, S> {
    log: NorFlashLog<'a, S>,
    share_slot: AbSlot<'a, S>,
}

impl<'a, S: NorFlash> MutationLog<'a, S> {
    pub fn new(
        mut share_flash: FlashPartition<'a, S>,
        mut log_flash: FlashPartition<'a, S>,
    ) -> Self {
        log_flash.tag = "event-log";
        share_flash.tag = "share";
        let share_slot = AbSlot::new(share_flash);
        MutationLog {
            log: NorFlashLog::new(log_flash),
            share_slot,
        }
    }

    pub fn push(&mut self, value: Mutation) -> Result<(), bincode::error::EncodeError> {
        match value {
            // For these mutations (which contain secret shares of some type) we don't write them to
            // the log, we write them to a special part of flash. We only store one secret share at a
            // time.
            Mutation::Core(device::Mutation::Keygen(device::keys::KeyMutation::SaveShare(save_share_mutation))) => {
                self.share_slot
                    .write(&ShareSlot::SecretShare(save_share_mutation));
            }
            Mutation::Core(device::Mutation::Restoration(
                device::restoration::RestorationMutation::Save(saved_backup),
            )) => {
                self.share_slot.write(&ShareSlot::SavedBackup(saved_backup));
            }
            value => {
                self.log.push(value)?;
            }
        }
        Ok(())
    }

    pub fn seek_iter(
        &mut self,
    ) -> impl Iterator<Item = Result<Mutation, bincode::error::DecodeError>> + use<'_, 'a, S> {
        self.log.seek_iter::<Mutation>().chain(
            self.share_slot
                .read::<ShareSlot>()
                .into_iter()
                .flat_map(|share_slot| share_slot.into_mutations())
                .map(Mutation::Core)
                .map(Ok),
        )
    }

    pub fn append(
        &mut self,
        iter: impl IntoIterator<Item = Mutation>,
    ) -> Result<(), bincode::error::EncodeError> {
        for item in iter {
            self.push(item)?;
        }
        Ok(())
    }
}
