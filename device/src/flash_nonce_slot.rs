use crate::storage::APP_STORAGE_END;
use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;
use embedded_storage::{ReadStorage, Storage};
use esp_storage::FlashStorage;
use frostsnap_core::device_nonces::*;

pub const FLASH_NONCE_SLOTS: usize = 16;
const SECTOR_SIZE: usize = 4096;
pub const FLASH_NONCE_SLOTS_SIZE: u32 = FLASH_NONCE_SLOTS as u32 * SECTOR_SIZE as u32 * 2;
const NONCE_FLASH_BINCODE_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Fixint,
    bincode::config::NoLimit,
> = bincode::config::standard().with_fixed_int_encoding();

#[derive(Debug)]
pub struct FlashNonceSlot {
    flash: Rc<RefCell<FlashStorage>>,
    offset: u32,
    /// A single write buf shared by all slots
    write_buf: Rc<RefCell<Vec<u8>>>,
}

impl NonceStreamSlot for FlashNonceSlot {
    fn read_index(&mut self) -> Option<u32> {
        let index = bincode::decode_from_reader::<u32, _, _>(
            BincodeFlashReader {
                flash: &mut self.flash.borrow_mut(),
                pos: self.offset,
            },
            NONCE_FLASH_BINCODE_CONFIG,
        )
        .expect("should always be able to read an int");

        if index == u32::MAX {
            None
        } else {
            Some(index)
        }
    }

    fn read_slot(&mut self) -> Option<SecretNonceSlot> {
        let value = bincode::decode_from_reader::<SecretNonceSlot, _, _>(
            BincodeFlashReader {
                flash: &mut self.flash.borrow_mut(),
                pos: self.offset,
            },
            NONCE_FLASH_BINCODE_CONFIG,
        )
        .ok()?;

        if value.index == u32::MAX {
            None
        } else {
            Some(value)
        }
    }

    fn write_slot(&mut self, value: &SecretNonceSlot) {
        let mut write_buf = self.write_buf.borrow_mut();
        write_buf.clear();
        bincode::encode_into_writer(value, BufWriter(&mut write_buf), NONCE_FLASH_BINCODE_CONFIG)
            .expect("will encode");
        assert!(write_buf.len() <= SECTOR_SIZE);
        assert!(value.index < u32::MAX, "u32::MAX is reserved to mean empty");
        self.flash
            .borrow_mut()
            .write(self.offset, &write_buf[..])
            .expect("must not fail to write");

        drop(write_buf);

        // slightly paranoid but we really gotta make sure the write happened
        assert_eq!(
            self.read_slot().as_ref(),
            Some(value),
            "flash failed to write correctly"
        );
    }
}

pub fn flash_nonce_slots(flash: Rc<RefCell<FlashStorage>>) -> ABSlots<FlashNonceSlot> {
    let mut slots = vec![];
    let write_buf = Rc::new(RefCell::new(vec![]));
    let mut curr = APP_STORAGE_END;
    for _ in 0..FLASH_NONCE_SLOTS {
        let offset1 = curr;
        curr += SECTOR_SIZE as u32;
        let offset2 = curr;
        curr += SECTOR_SIZE as u32;
        slots.push(ABSlot::new(
            FlashNonceSlot {
                flash: flash.clone(),
                offset: offset1,
                write_buf: write_buf.clone(),
            },
            FlashNonceSlot {
                flash: flash.clone(),
                offset: offset2,
                write_buf: write_buf.clone(),
            },
        ));
    }

    ABSlots::new(slots)
}

struct BufWriter<'a>(&'a mut Vec<u8>);

impl bincode::enc::write::Writer for BufWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        self.0.extend(bytes);
        Ok(())
    }
}

struct BincodeFlashReader<'a> {
    flash: &'a mut FlashStorage,
    pos: u32,
}

impl bincode::de::read::Reader for BincodeFlashReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), bincode::error::DecodeError> {
        self.flash.read(self.pos, bytes).map_err(|e| {
            bincode::error::DecodeError::OtherString(format!("Flash read error {:?}", e))
        })?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}
