// use core::num::NonZeroU8;
use esp_hal::efuse::{self as hal_efuse, Efuse};
use esp_hal::peripherals::EFUSE;
use rand_chacha::rand_core::RngCore;
use reed_solomon;

pub struct EfuseController {
    pub efuse: EFUSE,
}

impl EfuseController {
    pub fn new(efuse: EFUSE) -> Self {
        Self { efuse }
    }

    pub fn initialize_hmac_key(&self, rng: &mut impl RngCore) -> Result<(), EfuseError> {
        let key: [u8; 32] = Efuse::read_field_le(hal_efuse::KEY1);
        // Check if there's a key1 so we don't accidentally overwrite it
        if key.iter().eq([0u8; 32].iter()) {
            let mut buff = [0x00_u8; 32];
            rng.fill_bytes(&mut buff);

            unsafe {
                self.write_block(&buff, 5)?;
                self.write_key_purpose(1, KeyPurpose::HmacUpstream)?;
            }
        }
        Ok(())
    }

    /// # Safety
    ///
    /// Only write once to efuses
    unsafe fn write_key_purpose(
        &self,
        key_number: u8,
        key_purpose: KeyPurpose,
    ) -> Result<(), EfuseError> {
        let mut buff = [0x00u8; 32];

        let kp = key_purpose as u8;
        match key_number {
            0 => buff[11] = kp,
            1 => buff[11] = kp << 4,
            2 => buff[12] = kp,
            3 => buff[12] = kp << 4,
            4 => buff[13] = kp,
            5 => buff[13] = kp << 4,
            _ => return Err(EfuseError::EfuseError),
        }
        self.write_block(&buff, 0)
    }

    /// # Safety
    ///
    /// Only write once to efuses
    unsafe fn write_block(&self, data: &[u8; 32], block_number: u8) -> Result<(), EfuseError> {
        let efuse = &self.efuse;

        let mut to_burn: [u32; 11] = [0; 11];

        // Generate and write Reed-Solomon ECC
        // Fuse controller ignores RS code for blocks 0 and 1
        let rs_enc = reed_solomon::Encoder::new(12);
        let mut ecc = rs_enc.encode(data);

        // Flip efuse words to little endian
        for (i, word) in ecc.chunks_mut(4).enumerate() {
            let n = u32::from_le_bytes(word.try_into().unwrap());
            to_burn[i] = n;
        }

        // Write to efuse controller register
        efuse.pgm_data0().write(|w| w.bits(to_burn[0]));
        efuse.pgm_data1().write(|w| w.bits(to_burn[1]));
        efuse.pgm_data2().write(|w| w.bits(to_burn[2]));
        efuse.pgm_data3().write(|w| w.bits(to_burn[3]));
        efuse.pgm_data4().write(|w| w.bits(to_burn[4]));
        efuse.pgm_data5().write(|w| w.bits(to_burn[5]));
        efuse.pgm_data6().write(|w| w.bits(to_burn[6]));
        efuse.pgm_data7().write(|w| w.bits(to_burn[7]));

        efuse.pgm_check_value0().write(|w| w.bits(to_burn[8]));
        efuse.pgm_check_value1().write(|w| w.bits(to_burn[9]));
        efuse.pgm_check_value2().write(|w| w.bits(to_burn[10]));

        self.send_write_command(block_number);

        self.update_read_registers();

        if self.get_programming_error_record(block_number) {
            Err(EfuseError::EfuseWriteError(block_number))
        } else {
            Ok(())
        }
    }

    unsafe fn send_write_command(&self, block_number: u8) {
        let efuse = &self.efuse;

        // Send opcode, blknum and write command
        efuse.conf().write(|w| w.op_code().bits(0x5A5A));

        efuse
            .cmd()
            .write(|w| w.blk_num().bits(block_number).pgm_cmd().set_bit());
        // Poll command register until write bit is cleared
        while efuse.cmd().read().pgm_cmd().bit_is_set() {}

        // Clear efuse program and check registers
        efuse.pgm_data0().write(|w| w.bits(0));
        efuse.pgm_data1().write(|w| w.bits(0));
        efuse.pgm_data2().write(|w| w.bits(0));
        efuse.pgm_data3().write(|w| w.bits(0));
        efuse.pgm_data4().write(|w| w.bits(0));
        efuse.pgm_data5().write(|w| w.bits(0));
        efuse.pgm_data6().write(|w| w.bits(0));
        efuse.pgm_data7().write(|w| w.bits(0));

        efuse.pgm_check_value0().write(|w| w.bits(0));
        efuse.pgm_check_value1().write(|w| w.bits(0));
        efuse.pgm_check_value2().write(|w| w.bits(0));
    }

    fn get_programming_error_record(&self, block_number: u8) -> bool {
        let efuse = &self.efuse;
        match block_number {
            0 => {
                (efuse.rd_repeat_err1().read().bits() > 0)
                    || (efuse.rd_repeat_err2().read().bits() > 0)
            }
            4 => efuse.rd_rs_err0().read().key0_fail().bit(),
            5 => efuse.rd_rs_err0().read().key1_fail().bit(),
            6 => efuse.rd_rs_err0().read().key2_fail().bit(),
            7 => efuse.rd_rs_err0().read().key3_fail().bit(),
            8 => efuse.rd_rs_err0().read().key4_fail().bit(),
            9 => efuse.rd_rs_err1().read().key5_fail().bit(),
            _ => false,
        }
    }

    unsafe fn update_read_registers(&self) {
        let efuse = &self.efuse;

        // Send opcode and read command
        efuse.conf().write(|w| w.op_code().bits(0x5AA5));
        efuse.cmd().write(|w| w.read_cmd().set_bit());

        // Poll command register until read bit is cleared
        while efuse.cmd().read().read_cmd().bit_is_set() {}
    }
}

#[derive(Debug)]
pub enum KeyPurpose {
    User = 0,
    Aes128 = 4,
    HmacDownstream = 5,
    JtagHmacDownstream = 6,
    Ds = 7,
    HmacUpstream = 8,
    SecureBootDigest0 = 9,
    SecureBootDigest1 = 10,
    SecureBootDigest2 = 11,
}

#[derive(Debug)]
pub enum EfuseError {
    EfuseReadError,
    EfuseWriteError(u8),
    EfuseError,
}
