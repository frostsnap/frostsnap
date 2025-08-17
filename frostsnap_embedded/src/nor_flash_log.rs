use alloc::string::ToString;
use embedded_storage::nor_flash::NorFlash;

use crate::FlashPartition;
const WORD_SIZE: u32 = core::mem::size_of::<u32>() as u32;
// so we get some buffer exhaustion while writing if we're testing
const WRITE_BUF_SIZE: usize = if cfg!(debug_assertions) { 512 } else { 32 };

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
        core::iter::from_fn(move || {
            let mut length_buf = [0u8; WORD_SIZE as usize];
            let length_word_byte_pos = self.word_pos * WORD_SIZE;
            if let Err(e) = self.flash.read(length_word_byte_pos, &mut length_buf[..]) {
                return Some(Err(bincode::error::DecodeError::OtherString(format!(
                    "failed to read length byte at {length_word_byte_pos} ({:?}) from {:?}",
                    e, self.flash,
                ))));
            }
            if length_buf == [0xff; WORD_SIZE as usize] {
                return None;
            }
            self.word_pos += 1;
            let word_length = u32::from_le_bytes(length_buf);
            let mut reader = self.flash.bincode_reader();
            let body_byte_pos = self.word_pos * WORD_SIZE;
            reader.seek_byte(body_byte_pos);
            let result =
                bincode::decode_from_reader::<I, _, _>(&mut reader, bincode::config::standard());
            let expected_pos = body_byte_pos + word_length * WORD_SIZE;
            assert!(reader.byte_pos() <= expected_pos);
            self.word_pos += word_length;
            Some(result)
        })
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
    }
}
