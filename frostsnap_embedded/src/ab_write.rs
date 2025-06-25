use crate::FlashPartition;
use embedded_storage::nor_flash::NorFlash;
pub const ABWRITE_BINCODE_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Fixint,
    bincode::config::NoLimit,
> = bincode::config::standard().with_fixed_int_encoding();

/// Manages two writable sectors of persistent storage such that we make sure the state of the system we're managing is never lost.
/// The new state is first written, if that succeeds we finally write over the previous state.
#[derive(Clone, Debug)]
pub struct AbSlot<'a, S> {
    slots: [Slot<'a, S>; 2],
}

impl<'a, S: NorFlash> AbSlot<'a, S> {
    pub fn new(mut partition: FlashPartition<'a, S>) -> Self {
        assert!(partition.n_sectors() >= 2);
        assert_eq!(
            partition.n_sectors() % 2,
            0,
            "ab-write partition sector size must be divisible by 2"
        );
        let slot_size = partition.n_sectors() / 2;
        let b_slot = Slot {
            flash: partition.split_off_end(slot_size),
        };
        let a_slot = Slot { flash: partition };

        Self {
            slots: [a_slot, b_slot],
        }
    }

    pub fn write<T>(&self, value: &T)
    where
        T: bincode::Encode,
    {
        let (next_slot, next_index) = match self.current_slot_and_index() {
            Some((current_slot, current_index, _)) => {
                let next_slot = (current_slot + 1) % 2;
                let next_index = current_index + 1;
                if next_index == u32::MAX {
                    panic!("slot has been written too many times");
                }
                (next_slot, next_index)
            }
            None => (0, 0),
        };

        let slot_value = SlotValue {
            index: next_index,
            value,
        };
        let other_slot = (next_slot + 1) % 2;
        self.slots[next_slot].write(slot_value);
        self.slots[other_slot].write(slot_value);
    }

    pub fn read<T: bincode::Decode<()>>(&self) -> Option<T> {
        let current_slot = self.current_slot();
        let slot_value = self.slots[current_slot].read();
        slot_value.map(|slot_value| slot_value.value)
    }

    fn current_slot(&self) -> usize {
        if self.slots[1].read_index() > self.slots[0].read_index() {
            1
        } else {
            0
        }
    }

    fn current_slot_and_index(&self) -> Option<(usize, u32, bool)> {
        let a_index = self.slots[0].read_index()?;
        let b_index = self.slots[0].read_index()?;
        Some(if b_index > a_index {
            (1, b_index, b_index == a_index)
        } else {
            (0, a_index, b_index == a_index)
        })
    }

    pub fn current_index(&mut self) -> u32 {
        self.slots[0]
            .read_index()
            .max(self.slots[1].read_index())
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
struct Slot<'a, S> {
    flash: FlashPartition<'a, S>,
}

impl<S: NorFlash> Slot<'_, S> {
    // TODO: justify no erorr type here
    pub fn read<T: bincode::Decode<()>>(&self) -> Option<SlotValue<T>> {
        let value = bincode::decode_from_reader::<SlotValue<T>, _, _>(
            self.flash.bincode_reader(),
            ABWRITE_BINCODE_CONFIG,
        )
        .ok()?;

        if value.index == u32::MAX {
            None
        } else {
            Some(value)
        }
    }

    pub fn write<T: bincode::Encode>(&self, value: SlotValue<T>) {
        self.flash.erase_all().expect("must erase");
        let mut writer = self.flash.bincode_writer_remember_to_flush::<256>();
        bincode::encode_into_writer(&value, &mut writer, ABWRITE_BINCODE_CONFIG)
            .expect("will encode");
        writer.flush().expect("will flush all writes");
    }

    fn read_index(&self) -> Option<u32> {
        let index = bincode::decode_from_reader::<u32, _, _>(
            self.flash.bincode_reader(),
            ABWRITE_BINCODE_CONFIG,
        )
        .expect("should always be able to read an int");

        if index == u32::MAX {
            None
        } else {
            Some(index)
        }
    }
}

#[derive(Clone, Copy, Debug, bincode::Encode, bincode::Decode)]
struct SlotValue<T> {
    // the Sector with the newest index is chosen
    index: u32,
    value: T,
}
