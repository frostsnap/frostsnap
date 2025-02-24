use alloc::boxed::Box;
use embedded_storage::nor_flash;

pub struct TestNorFlash(pub Box<[u8; 4096 * 4]>);
const WORD_SIZE: u32 = 4;

impl Default for TestNorFlash {
    fn default() -> Self {
        Self::new()
    }
}

impl TestNorFlash {
    pub fn new() -> Self {
        Self(Box::new([0xffu8; 4096 * 4]))
    }
}

impl nor_flash::ErrorType for TestNorFlash {
    type Error = core::convert::Infallible;
}

impl nor_flash::ReadNorFlash for TestNorFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        bytes.copy_from_slice(&self.0[offset as usize..offset as usize + bytes.len()]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        4096 * 4
    }
}

impl nor_flash::NorFlash for TestNorFlash {
    const WRITE_SIZE: usize = WORD_SIZE as usize;
    const ERASE_SIZE: usize = 4096;

    fn erase(&mut self, _from: u32, _to: u32) -> Result<(), Self::Error> {
        todo!("not doing erase test yet")
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        assert!(offset % WORD_SIZE == 0);
        assert!(bytes.len() % 4 == 0);
        self.0[offset as usize..offset as usize + bytes.len()].copy_from_slice(bytes);

        Ok(())
    }
}
