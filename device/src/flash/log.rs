use alloc::{boxed::Box, string::String};
use embedded_storage::nor_flash::NorFlash;
use frostsnap_core::device::{self, SaveShareMutation};
use frostsnap_embedded::{AbSlot, FlashPartition, NorFlashLog};

// A card mutation carries share images, so a core mutation is now 244 B; it's only ever
// passed through on its way to or from flash, so it isn't boxed
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum Mutation {
    Core(device::Mutation),
    Name(String),
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum ShareSlot {
    SecretShare(Box<SaveShareMutation>),
    SavedBackup(device::restoration::SavedBackup),
    SavedBackup2(device::restoration::SavedBackup2),
    SavedBackup3(device::restoration::SavedBackup3),
}

impl ShareSlot {
    pub fn into_mutations(self) -> impl IntoIterator<Item = device::Mutation> {
        core::iter::once(match self {
            ShareSlot::SecretShare(save_share_mutation) => {
                device::Mutation::Keygen(device::keys::KeyMutation::SaveShare(save_share_mutation))
            }
            ShareSlot::SavedBackup(legacy_saved_backup) => device::Mutation::Restoration(
                device::restoration::RestorationMutation::Save(legacy_saved_backup),
            ),
            ShareSlot::SavedBackup2(saved_backup) => device::Mutation::Restoration(
                device::restoration::RestorationMutation::Save2(saved_backup),
            ),
            ShareSlot::SavedBackup3(saved_backup) => device::Mutation::Restoration(
                device::restoration::RestorationMutation::Save3(saved_backup),
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
        use device::restoration::RestorationMutation;
        let is_save3 = matches!(
            value,
            Mutation::Core(device::Mutation::Restoration(RestorationMutation::Save3(_)))
        );
        match value {
            // For these mutations (which contain secret shares of some type) we don't write them to
            // the log, we write them to a special part of flash. We only store one secret share at a
            // time.
            Mutation::Core(device::Mutation::Keygen(device::keys::KeyMutation::SaveShare(
                save_share_mutation,
            ))) => {
                self.share_slot
                    .write(&ShareSlot::SecretShare(save_share_mutation));
            }
            // Matched exhaustively, so a new kind of saved backup can't fall through to the
            // plaintext log
            Mutation::Core(device::Mutation::Restoration(restoration)) => {
                match restoration {
                    RestorationMutation::Save(legacy_saved_backup) => {
                        // Convert legacy SavedBackup to SavedBackup2 before storing.
                        // This code should never be reached. This code should never be
                        // reached.
                        let saved_backup: device::restoration::SavedBackup2 =
                            legacy_saved_backup.into();
                        self.share_slot
                            .write(&ShareSlot::SavedBackup2(saved_backup));
                    }
                    RestorationMutation::Save2(saved_backup) => {
                        self.share_slot
                            .write(&ShareSlot::SavedBackup2(saved_backup));
                    }
                    RestorationMutation::Save3(saved_backup) => {
                        self.share_slot
                            .write(&ShareSlot::SavedBackup3(saved_backup));
                    }
                    unsave @ RestorationMutation::_UnSave(_) => {
                        self.log
                            .push(Mutation::Core(device::Mutation::Restoration(unsave)))?;
                    }
                }
            }
            value => {
                self.log.push(value)?;
            }
        }
        // a card-restored share is saved in the share slot, never in the plaintext log
        debug_assert!(
            !is_save3
                || matches!(
                    self.share_slot.read::<ShareSlot>(),
                    Some(ShareSlot::SavedBackup3(_))
                ),
            "Save3 must be written to the share slot"
        );
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
