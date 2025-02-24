use alloc::vec::Vec;
use embedded_storage::nor_flash::NorFlash;
use frostsnap_core::device_nonces::{AbSlots, NonceStreamSlot, SecretNonceSlot};
use frostsnap_core::Versioned;

use crate::{ab_write::AbSlot, FlashPartition};

#[derive(Clone, Debug)]
pub struct NonceAbSlot<'a, S>(AbSlot<'a, S>);

impl<'a, S: NorFlash> NonceAbSlot<'a, S> {
    pub fn load_slots(mut partition: FlashPartition<'a, S>) -> AbSlots<Self> {
        let mut slots = Vec::with_capacity(partition.n_sectors() as usize / 2);
        while partition.n_sectors() >= 2 {
            slots.push(NonceAbSlot(AbSlot::new(partition.split_off_front(2))));
        }
        AbSlots::new(slots)
    }
}

impl<S: NorFlash> NonceStreamSlot for NonceAbSlot<'_, S> {
    fn read_slot_versioned(&mut self) -> Option<Versioned<SecretNonceSlot>> {
        self.0.read()
    }

    fn write_slot_versioned(&mut self, value: Versioned<&SecretNonceSlot>) {
        self.0.write(&value)
    }
}
