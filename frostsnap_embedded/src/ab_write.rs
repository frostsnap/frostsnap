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
            Some((current_slot, current_index)) => {
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
        let current_slot = self.current_slot_and_index().map_or(0, |(slot, _)| slot);
        let slot_value = self.slots[current_slot].read();
        slot_value.map(|slot_value| slot_value.value)
    }

    /// Picks the slot holding the newest write and its index. Both the read and
    /// write paths rely on this single source of truth: keeping the selection
    /// logic in one place is what stops the two paths from disagreeing about
    /// which slot is current (a disagreement is how a nonce index gets reused).
    fn current_slot_and_index(&self) -> Option<(usize, u32)> {
        let a_index = self.slots[0].read_index();
        let b_index = self.slots[1].read_index();
        // An empty slot (`None`) must rank below any written one, otherwise a
        // crash that leaves one slot mid-erase would make us forget the index
        // held in the other and reuse it.
        let current_slot = if b_index > a_index { 1 } else { 0 };
        Some((current_slot, a_index.max(b_index)?))
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::TestNorFlash;
    use core::cell::RefCell;
    use embedded_storage::nor_flash::{self, ReadNorFlash};
    use proptest::{collection, prelude::*};

    /// The highest index actually committed to flash, or `None` if both slots
    /// are empty. `Option<u32>` orders `None` below every `Some`, so this is
    /// exactly the value the recovery logic is supposed to track.
    fn committed_index<S: NorFlash>(ab: &AbSlot<'_, S>) -> Option<u32> {
        ab.slots[0]
            .read_index()
            .into_iter()
            .chain(ab.slots[1].read_index())
            .max()
    }

    #[test]
    fn write_roundtrip_advances_index() {
        let flash = RefCell::new(TestNorFlash::new());
        let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));

        ab.write(&100u32);
        assert_eq!(ab.read::<u32>(), Some(100));
        assert_eq!(committed_index(&ab), Some(0));

        ab.write(&200u32);
        assert_eq!(ab.read::<u32>(), Some(200));
        assert_eq!(committed_index(&ab), Some(1));
    }

    /// After an interrupted write the two slots end up at different indexes. A
    /// subsequent completed write must advance past the highest index ever
    /// committed and never reuse one — reusing an index here means reusing a
    /// nonce, which leaks the secret share. Run for both orientations so the
    /// test fails whichever slot the recovery logic is blind to.
    #[test]
    fn completed_write_after_torn_write_never_reuses_index() {
        for newer_slot in [0, 1] {
            let flash = RefCell::new(TestNorFlash::new());
            let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));

            ab.write(&100u32);
            ab.write(&200u32);

            // Power loss part way through writing 300: only one slot got the
            // new value, leaving the slots at different indexes. `write()` must
            // consult *both* slots to discover the true newest index.
            ab.slots[newer_slot].write(SlotValue {
                index: 2,
                value: &300u32,
            });

            assert_eq!(committed_index(&ab), Some(2));
            assert_eq!(ab.read::<u32>(), Some(300));

            ab.write(&400u32);

            assert_eq!(ab.read::<u32>(), Some(400));
            assert!(
                committed_index(&ab) > Some(2),
                "completed write reused index 2 (torn write left slot {newer_slot} newer)"
            );
        }
    }

    /// A crash can land after a slot is erased but before it is rewritten,
    /// leaving it empty while the other slot still holds the committed value.
    /// Recovery must keep the surviving value and must not reset the index to
    /// 0: a reset would give the next write a lower index than the stale slot,
    /// so a later interrupted write could win with old data still in place.
    #[test]
    fn recovers_when_one_slot_is_empty() {
        for empty_slot in [0, 1] {
            let flash = RefCell::new(TestNorFlash::new());
            let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));

            ab.write(&100u32);
            ab.write(&200u32);

            ab.slots[empty_slot].flash.erase_all().unwrap();

            assert_eq!(committed_index(&ab), Some(1));
            assert_eq!(ab.read::<u32>(), Some(200));

            ab.write(&300u32);

            assert_eq!(ab.read::<u32>(), Some(300));
            assert!(
                committed_index(&ab) > Some(1),
                "recovery from empty slot {empty_slot} reset the index instead of advancing it"
            );
        }
    }

    /// A flash that drops every erase/write after a configurable number of
    /// low-level operations, modelling power loss at an arbitrary point of a
    /// write. Reads always succeed against whatever actually landed.
    struct CrashFlash {
        inner: TestNorFlash,
        ops_until_crash: Option<u32>,
        tripped: bool,
    }

    impl CrashFlash {
        fn new() -> Self {
            Self {
                inner: TestNorFlash::new(),
                ops_until_crash: None,
                tripped: false,
            }
        }

        fn arm(&mut self, ops: u32) {
            self.ops_until_crash = Some(ops);
            self.tripped = false;
        }

        fn disarm(&mut self) {
            self.ops_until_crash = None;
        }

        fn allow_op(&mut self) -> bool {
            match &mut self.ops_until_crash {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    true
                }
                Some(_) => {
                    self.tripped = true;
                    false
                }
                None => true,
            }
        }
    }

    impl nor_flash::ErrorType for CrashFlash {
        type Error = core::convert::Infallible;
    }

    impl ReadNorFlash for CrashFlash {
        const READ_SIZE: usize = TestNorFlash::READ_SIZE;
        fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
            self.inner.read(offset, bytes)
        }
        fn capacity(&self) -> usize {
            self.inner.capacity()
        }
    }

    impl NorFlash for CrashFlash {
        const WRITE_SIZE: usize = TestNorFlash::WRITE_SIZE;
        const ERASE_SIZE: usize = TestNorFlash::ERASE_SIZE;
        fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
            if self.allow_op() {
                self.inner.erase(from, to)
            } else {
                Ok(())
            }
        }
        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
            if self.allow_op() {
                self.inner.write(offset, bytes)
            } else {
                Ok(())
            }
        }
    }

    proptest! {
        /// The core durability guarantee, exercised against crashes at every
        /// point of every write. Each round writes a strictly larger value, so
        /// "older value" and "smaller number" coincide. We assert what actually
        /// matters for nonce single-use: a fresh reader never sees a value (or
        /// index) older than one already committed — even mid-write — and a
        /// write that completes without crashing is fully visible with a brand
        /// new index. The value check also guards write ordering: overwriting
        /// the newest slot first would expose the stale slot here.
        #[test]
        fn read_never_returns_superseded_value_across_crashes(
            budgets in collection::vec(0u32..30, 1..30),
        ) {
            let flash = RefCell::new(CrashFlash::new());
            let reopen = || AbSlot::new(FlashPartition::new(&flash, 0, 2, "crash-test"));

            let mut committed_value: Option<u32> = None;
            let mut committed_index_floor: Option<u32> = None;

            for (round, budget) in budgets.into_iter().enumerate() {
                let value = round as u32;

                let ab = reopen();
                prop_assert!(
                    ab.read::<u32>() >= committed_value,
                    "stale value on reboot: {:?} < {committed_value:?}",
                    ab.read::<u32>()
                );
                prop_assert!(committed_index(&ab) >= committed_index_floor, "index regressed on reboot");

                flash.borrow_mut().arm(budget);
                reopen().write(&value);
                let tripped = flash.borrow().tripped;
                flash.borrow_mut().disarm();

                let ab = reopen();
                let seen = ab.read::<u32>();
                let seen_index = committed_index(&ab);
                prop_assert!(
                    seen >= committed_value,
                    "write exposed a superseded value: {seen:?} < {committed_value:?}"
                );
                prop_assert!(seen_index >= committed_index_floor, "write moved index backwards");

                if !tripped {
                    prop_assert_eq!(seen, Some(value), "completed write was not visible");
                    prop_assert!(seen_index > committed_index_floor, "completed write did not advance index");
                }
                // Whatever a fresh read now sees is durably committed: future
                // reads must never fall below it.
                if seen > committed_value {
                    committed_value = seen;
                }
                committed_index_floor = seen_index;
            }
        }
    }
}
