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

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.0[from as usize..to as usize].fill(0xff);
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        assert!(offset.is_multiple_of(WORD_SIZE));
        assert!(bytes.len().is_multiple_of(4));
        self.0[offset as usize..offset as usize + bytes.len()].copy_from_slice(bytes);

        Ok(())
    }
}
