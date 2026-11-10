use crate::FlashPartition;
use bincode::enc::write::{SizeWriter, Writer};
use embedded_storage::nor_flash::NorFlash;
pub const ABWRITE_BINCODE_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Fixint,
    bincode::config::NoLimit,
> = bincode::config::standard().with_fixed_int_encoding();

/// First word of a slot written in the checksummed layout:
///
/// `[SLOT_MAGIC][index][len][value: len bytes][crc32]`
///
/// Older firmware wrote `[index][value]` with no checksum. Those slots start
/// with their index, which counts up from 0 by one per write, so it can never
/// reach this value within the erase endurance of the flash.
const SLOT_MAGIC: u32 = 0xAB51_0701;
/// magic, index and len, one `u32` each
const HEADER_LEN: u32 = 12;
const CRC_LEN: u32 = 4;
static CRC: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

/// Manages two writable sectors of persistent storage such that we make sure the state of the system we're managing is never lost.
/// The new state is first written, if that succeeds we finally write over the previous state.
///
/// Each slot ends with a checksum written after the value, so a slot whose
/// write was interrupted by power loss is detected and the other slot is used
/// instead.
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
        let states = self.states();
        // Count indexes from interrupted writes too so the index never goes
        // backwards.
        let next_index = match states.iter().filter_map(SlotState::index).max() {
            Some(index) => {
                let next_index = index + 1;
                if next_index == u32::MAX {
                    panic!("slot has been written too many times");
                }
                next_index
            }
            None => 0,
        };

        // Never erase the newest readable slot first: if power is lost while
        // writing the other one, it is the only good copy left.
        let next_slot = match Self::newest_readable(&states) {
            Some(newest) => (newest + 1) % 2,
            None => 0,
        };
        let other_slot = (next_slot + 1) % 2;
        self.slots[next_slot].write(next_index, value);
        self.slots[other_slot].write(next_index, value);
    }

    pub fn read<T: bincode::Decode<()>>(&self) -> Option<T> {
        let states = self.states();
        let newest = Self::newest_readable(&states)?;
        let older = (newest + 1) % 2;
        // Fall back to the older slot if the newer one won't decode. This only
        // matters for old-layout slots, which have no checksum.
        self.slots[newest]
            .read_value(states[newest])
            .or_else(|| self.slots[older].read_value(states[older]))
    }

    fn states(&self) -> [SlotState; 2] {
        [self.slots[0].state(), self.slots[1].state()]
    }

    /// The slot holding the newest complete write. Both the read and write
    /// paths rely on this so they can't disagree about which slot is current.
    fn newest_readable(states: &[SlotState; 2]) -> Option<usize> {
        // `Option` ranks `None` below every `Some`, so an empty or torn slot is
        // older than any readable one.
        let a = states[0].readable_index();
        let b = states[1].readable_index();
        if b > a {
            Some(1)
        } else {
            a.map(|_| 0)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotState {
    Empty,
    /// Written by older firmware without a checksum, so it can't be verified.
    Legacy {
        index: u32,
    },
    /// Checksum verified.
    Committed {
        index: u32,
        len: u32,
    },
    /// Checksummed layout that failed verification: the write was interrupted.
    /// The index is kept (if it reached flash) so it is never reused.
    Torn {
        index: Option<u32>,
    },
}

impl SlotState {
    fn index(&self) -> Option<u32> {
        match *self {
            SlotState::Empty => None,
            SlotState::Legacy { index } | SlotState::Committed { index, .. } => Some(index),
            SlotState::Torn { index } => index,
        }
    }

    fn readable_index(&self) -> Option<u32> {
        match *self {
            SlotState::Legacy { index } | SlotState::Committed { index, .. } => Some(index),
            SlotState::Empty | SlotState::Torn { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
struct Slot<'a, S> {
    flash: FlashPartition<'a, S>,
}

impl<S: NorFlash> Slot<'_, S> {
    fn state(&self) -> SlotState {
        let mut header = [0u8; HEADER_LEN as usize];
        self.flash
            .read(0, &mut header)
            .expect("slot is large enough to hold the header");
        let word = |i: usize| u32::from_le_bytes(header[i * 4..i * 4 + 4].try_into().unwrap());
        let (first, index, len) = (word(0), word(1), word(2));

        match first {
            u32::MAX => SlotState::Empty,
            SLOT_MAGIC => {
                let index = (index != u32::MAX).then_some(index);
                let torn = SlotState::Torn { index };
                let Some(index) = index else {
                    return torn;
                };
                // also rejects an unwritten (0xffffffff) length
                if len > self.flash.size() - HEADER_LEN - CRC_LEN {
                    return torn;
                }

                let mut digest = CRC.digest();
                digest.update(&header);
                let mut buf = [0u8; 64];
                let end = HEADER_LEN + len;
                let mut pos = HEADER_LEN;
                while pos < end {
                    let n = (end - pos).min(buf.len() as u32);
                    let chunk = &mut buf[..n as usize];
                    self.flash.read(pos, chunk).expect("in bounds");
                    digest.update(chunk);
                    pos += n;
                }
                let mut stored_crc = [0u8; CRC_LEN as usize];
                self.flash.read(end, &mut stored_crc).expect("in bounds");

                if digest.finalize() == u32::from_le_bytes(stored_crc) {
                    SlotState::Committed { index, len }
                } else {
                    torn
                }
            }
            legacy_index => SlotState::Legacy {
                index: legacy_index,
            },
        }
    }

    fn read_value<T: bincode::Decode<()>>(&self, state: SlotState) -> Option<T> {
        match state {
            SlotState::Committed { len, .. } => {
                let mut reader = self.flash.bincode_reader();
                reader.seek_byte(HEADER_LEN);
                let value =
                    bincode::decode_from_reader::<T, _, _>(&mut reader, ABWRITE_BINCODE_CONFIG)
                        .ok()?;
                // the value must account for exactly the bytes that were written
                (reader.byte_pos() == HEADER_LEN + len).then_some(value)
            }
            SlotState::Legacy { .. } => bincode::decode_from_reader::<SlotValue<T>, _, _>(
                self.flash.bincode_reader(),
                ABWRITE_BINCODE_CONFIG,
            )
            .ok()
            .map(|slot_value| slot_value.value),
            SlotState::Empty | SlotState::Torn { .. } => None,
        }
    }

    fn write<T: bincode::Encode>(&self, index: u32, value: &T) {
        let mut size = SizeWriter::default();
        bincode::encode_into_writer(value, &mut size, ABWRITE_BINCODE_CONFIG).expect("will encode");
        let len = u32::try_from(size.bytes_written).expect("value fits in a u32 length");
        assert!(
            len <= self.flash.size() - HEADER_LEN - CRC_LEN,
            "value too large for ab-write slot"
        );

        self.flash.erase_all().expect("must erase");
        let mut writer = CrcWriter {
            inner: self.flash.bincode_writer_remember_to_flush::<256>(),
            digest: CRC.digest(),
        };
        for word in [SLOT_MAGIC, index, len] {
            writer.write(&word.to_le_bytes()).expect("will write");
        }
        bincode::encode_into_writer(value, &mut writer, ABWRITE_BINCODE_CONFIG)
            .expect("will encode");

        // The checksum goes last: it is what proves the rest was written.
        let CrcWriter { mut inner, digest } = writer;
        inner
            .write(&digest.finalize().to_le_bytes())
            .expect("will write");
        inner.flush().expect("will flush all writes");
    }
}

/// Passes bytes through to `inner` while adding them to a running checksum.
struct CrcWriter<W> {
    inner: W,
    digest: crc::Digest<'static, u32>,
}

impl<W: Writer> Writer for CrcWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        self.digest.update(bytes);
        self.inner.write(bytes)
    }
}

/// The layout written by older firmware: `[index][value]` with no checksum.
/// Only read now; the next write replaces it with the checksummed layout.
#[derive(Clone, Copy, Debug, bincode::Encode, bincode::Decode)]
struct SlotValue<T> {
    index: u32,
    value: T,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::TestNorFlash;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    /// The highest index of a complete write, or `None` if neither slot has
    /// one.
    fn committed_index<S: NorFlash>(ab: &AbSlot<'_, S>) -> Option<u32> {
        ab.states()
            .iter()
            .filter_map(SlotState::readable_index)
            .max()
    }

    fn assert_both_committed<S: NorFlash>(ab: &AbSlot<'_, S>) {
        for state in ab.states() {
            assert!(
                matches!(state, SlotState::Committed { .. }),
                "expected both slots committed, got {state:?}"
            );
        }
    }

    /// Writes a slot the way firmware did before checksums were added.
    fn write_legacy<S: NorFlash, T: bincode::Encode>(slot: &Slot<'_, S>, index: u32, value: T) {
        let mut flash = slot.flash;
        flash
            .erase_and_write_this::<256>(SlotValue { index, value })
            .unwrap();
    }

    /// Simulates power loss part way through writing `value` to `slot`: only
    /// the first `n_words` words of the encoded slot reach flash. Returns
    /// `None` once `n_words` covers the whole write.
    fn torn_write<S: NorFlash, T: bincode::Encode>(
        slot: &Slot<'_, S>,
        index: u32,
        value: &T,
        n_words: usize,
    ) -> Option<()> {
        slot.write(index, value);
        let mut full = vec![0u8; SECTOR_BYTES];
        slot.flash.read(0, &mut full).unwrap();
        let total_words = match slot.state() {
            SlotState::Committed { len, .. } => (HEADER_LEN + len + CRC_LEN).div_ceil(4) as usize,
            state => panic!("full write should commit, got {state:?}"),
        };
        if n_words >= total_words {
            return None;
        }
        slot.flash.erase_all().unwrap();
        slot.flash.nor_write(0, &full[..n_words * 4]).unwrap();
        Some(())
    }

    const SECTOR_BYTES: usize = 4096;

    #[test]
    fn write_roundtrip_advances_index() {
        let flash = RefCell::new(TestNorFlash::new());
        let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));

        assert_eq!(ab.read::<u32>(), None);

        ab.write(&100u32);
        assert_eq!(ab.read::<u32>(), Some(100));
        assert_eq!(committed_index(&ab), Some(0));

        ab.write(&200u32);
        assert_eq!(ab.read::<u32>(), Some(200));
        assert_eq!(committed_index(&ab), Some(1));
        assert_both_committed(&ab);
    }

    /// After an interrupted write the two slots end up at different indexes. A
    /// subsequent completed write must advance past the highest index ever
    /// committed and never reuse one. Run for both orientations so the test
    /// fails whichever slot the recovery logic is blind to.
    #[test]
    fn completed_write_after_torn_write_never_reuses_index() {
        for newer_slot in [0, 1] {
            let flash = RefCell::new(TestNorFlash::new());
            let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));

            ab.write(&100u32);
            ab.write(&200u32);

            // Power loss between the two slot writes of 300: only one slot got
            // the new value.
            ab.slots[newer_slot].write(2, &300u32);

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
    /// Recovery must keep the surviving value and must not reset the index.
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

    /// Issue #499: power loss part way through writing a single slot. Whatever
    /// prefix of the write reached flash (including just the index word), the
    /// slot must be ignored and the other slot's value returned. The next
    /// write must then succeed without reusing an index.
    fn check_torn_write_falls_back<T>(old: T, new: T, newer: T)
    where
        T: bincode::Encode + bincode::Decode<()> + PartialEq + core::fmt::Debug,
    {
        for torn_slot in [0, 1] {
            for n_words in 0.. {
                let flash = RefCell::new(TestNorFlash::new());
                let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));
                ab.write(&old);
                ab.write(&old);

                if torn_write(&ab.slots[torn_slot], 2, &new, n_words).is_none() {
                    break;
                }

                assert_eq!(
                    ab.read::<T>().as_ref(),
                    Some(&old),
                    "slot {torn_slot} torn after {n_words} words"
                );
                assert_eq!(committed_index(&ab), Some(1));

                ab.write(&newer);
                assert_eq!(ab.read::<T>().as_ref(), Some(&newer));
                assert!(committed_index(&ab) > Some(1));
                assert_both_committed(&ab);
            }
        }
    }

    #[test]
    fn torn_write_falls_back_to_other_slot() {
        check_torn_write_falls_back(200u32, 300u32, 400u32);
        // a value spanning several flush buffers
        check_torn_write_falls_back(vec![1u8; 300], vec![2u8; 600], vec![3u8; 10]);
    }

    /// A tear that leaves the newest slot complete except for a flipped bit
    /// (or any other corruption) is caught by the checksum.
    #[test]
    fn corrupted_slot_falls_back_to_other_slot() {
        for corrupt_slot in [0, 1] {
            let flash = RefCell::new(TestNorFlash::new());
            let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));
            ab.write(&vec![7u8; 40]);
            ab.slots[corrupt_slot].write(2, &vec![8u8; 40]);

            let offset = ab.slots[corrupt_slot].flash.size() as usize * corrupt_slot
                + HEADER_LEN as usize
                + 20;
            flash.borrow_mut().0[offset] ^= 0x01;

            assert_eq!(ab.read::<Vec<u8>>(), Some(vec![7u8; 40]));
        }
    }

    /// Devices in the field hold slots in the old `[index][value]` layout.
    /// They must still read back, and the next write moves both slots to the
    /// checksummed layout with a higher index.
    #[test]
    fn reads_and_migrates_legacy_slots() {
        for newer_slot in [0, 1] {
            let flash = RefCell::new(TestNorFlash::new());
            let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));
            write_legacy(&ab.slots[newer_slot], 6, 600u32);
            write_legacy(&ab.slots[(newer_slot + 1) % 2], 5, 500u32);

            assert_eq!(ab.read::<u32>(), Some(600));
            assert_eq!(committed_index(&ab), Some(6));

            ab.write(&700u32);
            assert_eq!(ab.read::<u32>(), Some(700));
            assert_eq!(committed_index(&ab), Some(7));
            assert_both_committed(&ab);
        }
    }

    /// The first checksummed write onto legacy slots is interrupted: the
    /// legacy value must survive.
    #[test]
    fn torn_migration_keeps_legacy_value() {
        for torn_slot in [0, 1] {
            for n_words in 0.. {
                let flash = RefCell::new(TestNorFlash::new());
                let ab = AbSlot::new(FlashPartition::new(&flash, 0, 2, "ab-test"));
                write_legacy(&ab.slots[0], 5, 500u32);
                write_legacy(&ab.slots[1], 5, 500u32);

                if torn_write(&ab.slots[torn_slot], 6, &600u32, n_words).is_none() {
                    break;
                }

                assert_eq!(ab.read::<u32>(), Some(500), "torn after {n_words} words");

                ab.write(&700u32);
                assert_eq!(ab.read::<u32>(), Some(700));
                assert!(committed_index(&ab) > Some(5));
                assert_both_committed(&ab);
            }
        }
    }
}
