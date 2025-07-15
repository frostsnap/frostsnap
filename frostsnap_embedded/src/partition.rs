use core::cell::RefCell;
use embedded_storage::nor_flash::{NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash};

use crate::ABWRITE_BINCODE_CONFIG;

pub struct FlashPartition<'a, S> {
    pub tag: &'static str,
    offset_sector: u32,
    n_sectors: u32,
    flash: &'a RefCell<S>,
}

impl<S> core::fmt::Debug for FlashPartition<'_, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FlashPartition")
            .field("tag", &self.tag)
            .field("offset_sector", &self.offset_sector)
            .field("n_sectors", &self.n_sectors)
            .finish()
    }
}

// Clone won't derive for some reason
impl<S> Clone for FlashPartition<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for FlashPartition<'_, S> {}
pub const SECTOR_SIZE: usize = 4096;

impl<'a, S: NorFlash> FlashPartition<'a, S> {
    pub fn new(
        flash: &'a RefCell<S>,
        offset_sector: u32,
        n_sectors: u32,
        tag: &'static str,
    ) -> Self {
        Self {
            tag,
            offset_sector,
            n_sectors,
            flash,
        }
    }

    pub fn nor_write(&self, offset: u32, bytes: &[u8]) -> Result<(), NorFlashErrorKind> {
        if offset.saturating_add(bytes.len() as u32) > self.n_sectors * SECTOR_SIZE as u32 {
            return Err(NorFlashErrorKind::OutOfBounds);
        }

        let abs_offset = offset + self.offset_sector * SECTOR_SIZE as u32;

        NorFlash::write(&mut *self.flash.borrow_mut(), abs_offset, bytes).map_err(|e| e.kind())?;

        Ok(())
    }

    pub fn nor_write_sector(
        &self,
        sector: u32,
        bytes: &[u8; SECTOR_SIZE],
    ) -> Result<(), NorFlashErrorKind> {
        self.nor_write(sector * SECTOR_SIZE as u32, &bytes[..])
    }

    pub fn read(&self, offset: u32, bytes: &mut [u8]) -> Result<(), NorFlashErrorKind> {
        if offset.saturating_add(bytes.len() as u32) > self.n_sectors * SECTOR_SIZE as u32 {
            return Err(NorFlashErrorKind::OutOfBounds);
        }
        let abs_offset = offset + self.offset_sector * SECTOR_SIZE as u32;

        ReadNorFlash::read(&mut *self.flash.borrow_mut(), abs_offset, bytes)
            .map_err(|e| e.kind())?;
        Ok(())
    }

    pub fn read_sector(&self, sector: u32) -> Result<[u8; SECTOR_SIZE], NorFlashErrorKind> {
        let mut ret = [0u8; SECTOR_SIZE];
        self.read(sector * SECTOR_SIZE as u32, &mut ret[..])?;
        Ok(ret)
    }

    /// splits n_sectors off the end of the parition into a new parition
    pub fn split_off_end(&mut self, n_sectors: u32) -> FlashPartition<'a, S> {
        assert!(n_sectors <= self.n_sectors);
        self.n_sectors -= n_sectors;
        let new_offset_sector = self.offset_sector + self.n_sectors;

        FlashPartition {
            tag: self.tag,
            offset_sector: new_offset_sector,
            n_sectors,
            flash: self.flash,
        }
    }

    /// splits n_sectors off the front of the parition into a new partition
    pub fn split_off_front(&mut self, n_sectors: u32) -> FlashPartition<'a, S> {
        assert!(n_sectors <= self.n_sectors);
        let mut end = self.split_off_end(self.n_sectors - n_sectors);
        // make the end the front
        core::mem::swap(self, &mut end);
        end
    }

    pub fn erase_sector(&self, sector: u32) -> Result<(), NorFlashErrorKind> {
        if sector >= self.n_sectors {
            return Err(NorFlashErrorKind::OutOfBounds);
        }
        let sector = self.offset_sector + sector;

        NorFlash::erase(
            &mut *self.flash.borrow_mut(),
            sector * SECTOR_SIZE as u32,
            (sector + 1) * SECTOR_SIZE as u32,
        )
        .map_err(|e| e.kind())
    }

    pub fn erase_all(&self) -> Result<(), NorFlashErrorKind> {
        let start = self.offset_sector * SECTOR_SIZE as u32;
        NorFlash::erase(
            &mut *self.flash.borrow_mut(),
            start,
            start + self.n_sectors * SECTOR_SIZE as u32,
        )
        .map_err(|e| e.kind())
    }

    pub fn is_empty(&self) -> Result<bool, NorFlashErrorKind> {
        for sector in 0..self.n_sectors {
            let data = self.read_sector(sector)?;
            if data.iter().any(|byte| *byte != 0xff) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn set_offset(&mut self, offset: u32) {
        assert_eq!(offset % SECTOR_SIZE as u32, 0);
        self.offset_sector = offset / SECTOR_SIZE as u32;
    }

    pub fn set_size(&mut self, size: u32) {
        assert_eq!(size % SECTOR_SIZE as u32, 0);
        self.n_sectors = size / SECTOR_SIZE as u32;
    }

    pub fn set_offset_and_size(&mut self, offset: u32, size: u32) {
        self.set_size(size);
        self.set_offset(offset);
    }

    pub fn n_sectors(&self) -> u32 {
        self.n_sectors
    }

    /// size in bytes
    pub fn size(&self) -> u32 {
        self.n_sectors * SECTOR_SIZE as u32
    }

    pub fn bincode_reader(&self) -> BincodeFlashReader<'a, S> {
        BincodeFlashReader {
            flash: *self,
            pos: 0,
        }
    }

    pub fn bincode_writer_remember_to_flush<const BUFFER_SIZE: usize>(
        &self,
    ) -> BincodeFlashWriter<'a, S, BUFFER_SIZE> {
        assert_eq!(BUFFER_SIZE % S::WRITE_SIZE, 0);
        assert_eq!(S::ERASE_SIZE % BUFFER_SIZE, 0);
        BincodeFlashWriter {
            flash: *self,
            buf: [0xff; BUFFER_SIZE],
            buf_index: 0,
            word_pos: 0,
        }
    }

    pub fn erase_and_write_this<const BUFFER_SIZE: usize>(
        &mut self,
        blob: impl bincode::Encode,
    ) -> Result<u32, NorFlashErrorKind> {
        self.erase_all()?;
        let mut writer = self.bincode_writer_remember_to_flush::<BUFFER_SIZE>();
        // FIXME: it's a bit annoying no error message can be passed into this error kind
        bincode::encode_into_writer(blob, &mut writer, ABWRITE_BINCODE_CONFIG)
            .map_err(|_| NorFlashErrorKind::Other)?;
        let bytes_written = writer.flush()?;
        Ok(bytes_written)
    }
}

pub struct BincodeFlashReader<'a, S> {
    flash: FlashPartition<'a, S>,
    pos: u32,
}

impl<S> BincodeFlashReader<'_, S> {
    pub fn seek_byte(&mut self, pos: u32) {
        self.pos = pos;
    }

    pub fn byte_pos(&self) -> u32 {
        self.pos
    }
}

impl<S: NorFlash> bincode::de::read::Reader for BincodeFlashReader<'_, S> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), bincode::error::DecodeError> {
        // this only works because we're using the "bytewise-read" feature of esp-storage
        self.flash.read(self.pos, bytes).map_err(|e| {
            bincode::error::DecodeError::OtherString(format!("Flash read error {e:?}"))
        })?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct BincodeFlashWriter<'a, S, const BUFFER_SIZE: usize> {
    flash: FlashPartition<'a, S>,
    buf: [u8; BUFFER_SIZE],
    buf_index: usize,
    word_pos: u32,
}

impl<S: NorFlash, const BUFFER_SIZE: usize> BincodeFlashWriter<'_, S, BUFFER_SIZE> {
    pub fn seek_word(&mut self, word: u32) {
        self.word_pos = word;
    }

    pub fn curr_word(&self) -> u32 {
        self.word_pos
    }

    pub fn flush(mut self) -> Result<u32, NorFlashErrorKind> {
        if self.buf_index != 0 {
            let aligned_index = self.buf_index
                + ((S::WRITE_SIZE - (self.buf_index % S::WRITE_SIZE)) % S::WRITE_SIZE);
            self.buf[self.buf_index..aligned_index].fill(0xff);
            // set to zero so we don't get the drop panic even if we fail
            self.buf_index = 0;
            self.flash
                .nor_write(
                    self.word_pos * S::WRITE_SIZE as u32,
                    &self.buf[..aligned_index],
                )
                .map_err(|e| e.kind())?;
            self.word_pos += aligned_index as u32 / S::WRITE_SIZE as u32;
        }

        Ok(self.word_pos)
    }
}

impl<S: NorFlash, const BUFFER_SIZE: usize> bincode::enc::write::Writer
    for BincodeFlashWriter<'_, S, BUFFER_SIZE>
{
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        let mut i = 0;
        loop {
            if i == bytes.len() {
                break;
            }

            self.buf[self.buf_index] = bytes[i];
            self.buf_index += 1;

            if self.buf_index == BUFFER_SIZE {
                self.flash
                    .nor_write(self.word_pos * S::WRITE_SIZE as u32, &self.buf[..])
                    .map_err(|e| bincode::error::EncodeError::OtherString(format!("{e:?}")))?;
                self.buf_index = 0;
                self.word_pos += (BUFFER_SIZE / S::WRITE_SIZE) as u32;
            }

            i += 1;
        }

        Ok(())
    }
}

impl<S, const BUFFER_SIZE: usize> Drop for BincodeFlashWriter<'_, S, BUFFER_SIZE> {
    fn drop(&mut self) {
        assert_eq!(
            self.buf_index, 0,
            "BincodeFlashWriter must be empty when dropped"
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::TestNorFlash;
    use core::cell::RefCell;
    use proptest::{collection, prelude::*};

    #[test]
    fn split_off_front() {
        let test = RefCell::new(TestNorFlash::new());
        let mut partition = FlashPartition::new(&test, 1, 3, "test");

        let new_from_front = partition.split_off_front(1);
        new_from_front.nor_write(8, [42; 4].as_slice()).unwrap();
        partition.nor_write(8, [84; 4].as_slice()).unwrap();
        assert_eq!(&test.borrow().0[4096 + 8..4096 + 8 + 4], [42; 4].as_slice());
        assert_eq!(
            &test.borrow().0[4096 * 2 + 8..4096 * 2 + 8 + 4],
            [84; 4].as_slice()
        );
    }

    #[test]
    fn split_off_end() {
        let test = RefCell::new(TestNorFlash::new());
        let mut partition = FlashPartition::new(&test, 1, 3, "test");

        let new_from_back = partition.split_off_end(1);
        new_from_back.nor_write(8, [42; 4].as_slice()).unwrap();
        partition.nor_write(8, [84; 4].as_slice()).unwrap();
        assert_eq!(
            &test.borrow().0[4096 * 3 + 8..4096 * 3 + 8 + 4],
            [42; 4].as_slice()
        );
        assert_eq!(&test.borrow().0[4096 + 8..4096 + 8 + 4], [84; 4].as_slice());
    }

    proptest! {
        #[test]
        fn bincode_writer(data in collection::vec(any::<u8>(), 0..1024)) {
            let test = RefCell::new(TestNorFlash::new());
            let partition = FlashPartition::new(&test, 1, 3, "test");
            let mut writer = partition.bincode_writer_remember_to_flush::<32>();
            bincode::encode_into_writer(data.clone(), &mut writer, bincode::config::legacy() /* for fixint */).unwrap();
            let end = writer.flush().unwrap();
            prop_assert_eq!(end as usize, data.len().div_ceil(TestNorFlash::WRITE_SIZE) + /*int length is 8 bytes*/ 8 / TestNorFlash::WRITE_SIZE);
            prop_assert_eq!(&test.borrow().0[4096 + 8..4096 + 8 + data.len()], &data[..]);
        }
    }
}
