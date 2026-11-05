use crate::flash::HEADER_SECTORS;
use crate::ui::{self, UserInteraction};
use frostsnap_embedded::FlashPartition;
use frostsnap_widgets::Frac;

pub use frostsnap_embedded::ErasePoll;

/// Wipes NVS, showing progress on the device screen. The state machine itself lives in
/// [`frostsnap_embedded::Erase`] so it can be tested off-device.
pub struct Erase(frostsnap_embedded::Erase);

impl Erase {
    pub fn new<S>(partition: &FlashPartition<S>) -> Self
    where
        S: embedded_storage::nor_flash::NorFlash,
    {
        Self(frostsnap_embedded::Erase::new(partition, HEADER_SECTORS))
    }

    pub fn poll<S>(
        &mut self,
        partition: &FlashPartition<S>,
        ui: &mut impl UserInteraction,
    ) -> ErasePoll
    where
        S: embedded_storage::nor_flash::NorFlash,
    {
        self.0.poll(partition, |erased, total| {
            ui.set_workflow(ui::Workflow::EraseProgress {
                progress: Frac::from_ratio(erased, total),
            });
            ui.poll();
        })
    }
}
