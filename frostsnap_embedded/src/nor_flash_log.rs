use alloc::string::ToString;
use embedded_storage::nor_flash::{NorFlash, NorFlashErrorKind};

use crate::FlashPartition;
const WORD_SIZE: u32 = core::mem::size_of::<u32>() as u32;
// so we get some buffer exhaustion while writing if we're testing
pub const WRITE_BUF_SIZE: usize = if cfg!(debug_assertions) { 512 } else { 32 };

pub struct NorFlashLog<'a, S> {
    flash: FlashPartition<'a, S>,
    word_pos: u32,
}

impl<'a, S: NorFlash> NorFlashLog<'a, S> {
    pub fn new(flash: FlashPartition<'a, S>) -> Self {
        assert_eq!(WORD_SIZE, S::WRITE_SIZE as u32);
        Self { flash, word_pos: 0 }
    }

    /// The layout of the entries are word aligned and length prefixed. So the first entry has a
    /// four byte little endian entry length (in words), and then the main body of the item bincode
    /// encoded. Bincode doesn't care if it doesn't use all the bytes so there's no need to know
    /// exactly where it ends.
    pub fn push<I: bincode::Encode>(&mut self, item: I) -> Result<(), bincode::error::EncodeError> {
        let mut writer = self
            .flash
            .bincode_writer_remember_to_flush::<WRITE_BUF_SIZE>();
        // skip the first word because that's where we'll write the length
        let start_word = self.word_pos + 1;
        writer.seek_word(start_word);
        bincode::encode_into_writer(item, &mut writer, bincode::config::standard())?;
        let final_word = writer
            .flush()
            .map_err(|e| bincode::error::EncodeError::OtherString(e.to_string()))?;

        let written_word_length = final_word - start_word;
        let length_bytes = written_word_length.to_le_bytes();
        self.flash
            .nor_write(self.word_pos * WORD_SIZE, &length_bytes)
            .map_err(|e| bincode::error::EncodeError::OtherString(e.to_string()))?;
        self.word_pos += written_word_length + /* the length word */1;

        Ok(())
    }

    pub fn seek_iter<I: bincode::Decode<()>>(
        &mut self,
    ) -> impl Iterator<Item = Result<I, bincode::error::DecodeError>> + use<'_, 'a, S, I> {
        self.word_pos = 0;
        core::iter::from_fn(move || loop {
            let mut length_buf = [0u8; WORD_SIZE as usize];
            let length_word_byte_pos = self.word_pos * WORD_SIZE;
            if let Err(e) = self.flash.read(length_word_byte_pos, &mut length_buf[..]) {
                return Some(Err(bincode::error::DecodeError::OtherString(format!(
                    "failed to read length byte at {length_word_byte_pos} ({:?}) from {:?}",
                    e, self.flash,
                ))));
            }
            let mut word_length = u32::from_le_bytes(length_buf);
            // padding left by `zero_torn_tail`
            if word_length == 0 {
                self.word_pos += 1;
                continue;
            }
            if length_buf == [0xff; WORD_SIZE as usize] {
                if let Err(e) = self.zero_torn_tail() {
                    return Some(Err(bincode::error::DecodeError::OtherString(format!(
                        "failed to zero torn log tail ({e:?}) in {:?}",
                        self.flash,
                    ))));
                }
                return None;
            }
            self.word_pos += 1;
            let mut reader = self.flash.bincode_reader();
            let body_byte_pos = self.word_pos * WORD_SIZE;
            reader.seek_byte(body_byte_pos);
            let result =
                bincode::decode_from_reader::<I, _, _>(&mut reader, bincode::config::standard());
            if word_length > self.flash.size() / WORD_SIZE - self.word_pos {
                // Losing power while writing the last entry's length word leaves the real length
                // with extra bits set, and writing the real length over it clears them. Only do that
                // when the entry looks exactly like that, so that a bad read or a faded bit can't
                // send us writing into saved entries.
                let real_length = (reader.byte_pos() - body_byte_pos).div_ceil(WORD_SIZE);
                let repairable = result.is_ok()
                    && (word_length & real_length) == real_length
                    && matches!(self.programmed_end(self.word_pos + real_length), Ok(None));
                if !repairable {
                    return Some(Err(bincode::error::DecodeError::OtherString(format!(
                        "length {word_length} at {length_word_byte_pos} runs past the end of {:?}",
                        self.flash,
                    ))));
                }
                if let Err(e) = self
                    .flash
                    .nor_write(length_word_byte_pos, &real_length.to_le_bytes())
                {
                    return Some(Err(bincode::error::DecodeError::OtherString(format!(
                        "failed to repair length at {length_word_byte_pos} ({e:?}) in {:?}",
                        self.flash,
                    ))));
                }
                word_length = real_length;
            }
            let expected_pos = body_byte_pos + word_length * WORD_SIZE;
            assert!(reader.byte_pos() <= expected_pos);
            self.word_pos += word_length;
            return Some(result);
        })
    }

    /// Zeroes whatever a push that lost power while writing its body left after the end of the log,
    /// so the next push lands on erased flash. The word at the end of the log is zeroed last so that
    /// losing power in here leaves the log ending in the same place.
    fn zero_torn_tail(&mut self) -> Result<(), NorFlashErrorKind> {
        let Some(dirty_end) = self.programmed_end(self.word_pos)? else {
            return Ok(());
        };
        // Read the end of the log again so a one-off bad read of a committed length can't wipe the
        // entries after it.
        let mut again = [0u8; WORD_SIZE as usize];
        self.flash.read(self.word_pos * WORD_SIZE, &mut again)?;
        if again != [0xff; WORD_SIZE as usize] {
            return Err(NorFlashErrorKind::Other);
        }
        let zeros = [0u8; 256];
        let mut word = self.word_pos + 1;
        while word < dirty_end {
            let n_words = (dirty_end - word).min(zeros.len() as u32 / WORD_SIZE);
            self.flash
                .nor_write(word * WORD_SIZE, &zeros[..(n_words * WORD_SIZE) as usize])?;
            word += n_words;
        }
        self.flash
            .nor_write(self.word_pos * WORD_SIZE, &zeros[..WORD_SIZE as usize])?;
        self.word_pos = dirty_end;
        Ok(())
    }

    /// One past the last word at or after `word` that isn't erased, if there is one.
    fn programmed_end(&self, word: u32) -> Result<Option<u32>, NorFlashErrorKind> {
        let mut buf = [0u8; 256];
        let mut end = None;
        let mut pos = word * WORD_SIZE;
        while pos < self.flash.size() {
            let chunk = &mut buf[..(self.flash.size() - pos).min(256) as usize];
            self.flash.read(pos, chunk)?;
            if let Some(i) = chunk.iter().rposition(|byte| *byte != 0xff) {
                end = Some((pos + i as u32) / WORD_SIZE + 1);
            }
            pos += chunk.len() as u32;
        }
        Ok(end)
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod test {
    use crate::test::TestNorFlash;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use proptest::collection;
    use proptest::prelude::*;

    use crate::FlashPartition;

    use super::NorFlashLog;

    #[test]
    fn append_strings() {
        let test = RefCell::new(TestNorFlash::new());
        let mut log = NorFlashLog::new(FlashPartition::new(&test, 1, 3, "test"));
        log.push(["ab".to_string()]).unwrap();
        assert_eq!(
            log.seek_iter::<String>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec!["ab".to_string()]
        );

        log.push(["cde".to_string()]).unwrap();

        assert_eq!(
            log.seek_iter::<String>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec!["ab".to_string(), "cde".to_string()]
        );
        log.push([String::new()]).unwrap();

        assert_eq!(
            log.seek_iter::<String>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec!["ab".to_string(), "cde".to_string(), "".to_string()]
        );
    }

    proptest! {
        #[test]
        fn proptest_push(byte_vecs in collection::vec(collection::vec(any::<u8>(), 0..100), 0..100)) {
            let test = RefCell::new(TestNorFlash::new());
            let mut log = NorFlashLog::new(FlashPartition::new(&test, 1, 3, "test"));

            for byte_vec in byte_vecs.clone() {
                log.push(byte_vec).unwrap();
            }

            let got_byte_vecs = log.seek_iter::<Vec<u8>>().collect::<Result<Vec<_>, _>>().unwrap();
            prop_assert_eq!(byte_vecs, got_byte_vecs);
        }

        #[test]
        fn proptest_power_cut_during_push(
            committed in collection::vec(collection::vec(any::<u8>(), 0..100), 0..50),
            torn in collection::vec(any::<u8>(), 0..100),
            length_word_torn in any::<bool>(),
            bytes_written in any::<prop::sample::Index>(),
            junk_length_bits in any::<u32>(),
            after in collection::vec(any::<u8>(), 0..100),
        ) {
            let test = RefCell::new(TestNorFlash::new());
            let mut log = NorFlashLog::new(FlashPartition::new(&test, 1, 3, "test"));
            for byte_vec in committed.clone() {
                log.push(byte_vec).unwrap();
            }

            // lose power while pushing `torn`, either part way through the body or while writing
            // the length word after it
            let body = bincode::encode_to_vec(&torn, bincode::config::standard()).unwrap();
            let body_start = 4096 + (log.word_pos as usize + 1) * 4;
            {
                let flash = &mut test.borrow_mut().0;
                let n = if length_word_torn { body.len() } else { bytes_written.index(body.len() + 1) };
                flash[body_start..body_start + n].copy_from_slice(&body[..n]);
                if length_word_torn {
                    let torn_length = body.len().div_ceil(4) as u32 | junk_length_bits | 1 << 31;
                    flash[body_start - 4..body_start].copy_from_slice(&torn_length.to_le_bytes());
                }
            }

            // a torn length word gets repaired, while a torn body is dropped
            let mut expected = committed;
            if length_word_torn {
                expected.push(torn);
            }
            let got = log.seek_iter::<Vec<u8>>().collect::<Result<Vec<_>, _>>().unwrap();
            prop_assert_eq!(&got, &expected);

            let before = test.borrow().0.clone();
            log.push(after.clone()).unwrap();
            let wrote_over_programmed = before
                .iter()
                .zip(test.borrow().0.iter())
                .any(|(before, after)| before != after && *before != 0xff);
            prop_assert!(!wrote_over_programmed);

            expected.push(after);
            let got = log.seek_iter::<Vec<u8>>().collect::<Result<Vec<_>, _>>().unwrap();
            prop_assert_eq!(got, expected);
        }

        #[test]
        fn proptest_bad_length_before_saved_entries(
            committed in collection::vec(collection::vec(any::<u8>(), 0..100), 2..50),
            entry in any::<prop::sample::Index>(),
            junk_length_bits in any::<u32>(),
        ) {
            let test = RefCell::new(TestNorFlash::new());
            let mut log = NorFlashLog::new(FlashPartition::new(&test, 1, 3, "test"));
            let mut length_word_positions = vec![];
            for byte_vec in committed.clone() {
                length_word_positions.push(4096 + log.word_pos as usize * 4);
                log.push(byte_vec).unwrap();
            }

            // bits fade in the length word of an entry that isn't the last one
            let pos = length_word_positions[entry.index(committed.len() - 1)];
            {
                let flash = &mut test.borrow_mut().0;
                let length = u32::from_le_bytes(flash[pos..pos + 4].try_into().unwrap());
                flash[pos..pos + 4].copy_from_slice(&(length | junk_length_bits | 1 << 31).to_le_bytes());
            }

            // replay fails and leaves the flash alone rather than writing over saved entries
            let before = test.borrow().0.clone();
            prop_assert!(log.seek_iter::<Vec<u8>>().collect::<Result<Vec<_>, _>>().is_err());
            prop_assert!(test.borrow().0 == before);
        }
    }
}
