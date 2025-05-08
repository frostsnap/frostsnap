use esp_hal::efuse::{self as hal_efuse, Efuse};
use esp_hal::peripherals::EFUSE;
use frostsnap_core::AccessStructureRef;
use rand_chacha::rand_core::RngCore;
use rand_core::SeedableRng;
use reed_solomon;

const KEY_BLOCKS_OFFSET: u8 = 4;
const WR_DIS_KEY_OFFSET: u8 = 23;
const WR_DIS_KP_OFFSET: u8 = 8;
const READ_COMMAND: u16 = 0x5AA5;
const WRITE_COMMAND: u16 = 0x5A5A;

pub struct EfuseController {
    pub efuse: EFUSE,
}

impl EfuseController {
    pub fn new(efuse: EFUSE) -> Self {
        Self { efuse }
    }

    pub fn randomly_init_key(
        &self,
        key_id: u8,
        key_purpose: KeyPurpose,
        read_protect: bool,
        rng: &mut impl RngCore,
    ) -> Result<Option<[u8; 32]>, EfuseError> {
        if !Self::is_key_written(key_id) {
            let mut buf = [0x00_u8; 32];
            rng.fill_bytes(&mut buf);
            self.set_efuse_key(key_id, key_purpose, read_protect, buf);
            return Ok(Some(buf));
        }

        Ok(None)
    }

    pub fn set_efuse_key(
        &self,
        key_id: u8,
        key_purpose: KeyPurpose,
        read_protect: bool,
        value: [u8; 32],
    ) -> Result<(), EfuseError> {
        if Self::is_key_written(key_id) {
            return Err(EfuseError::EfuseAlreadyBurned);
        }
        unsafe {
            self.write_block(&value, key_id + KEY_BLOCKS_OFFSET)?;
            self.write_key_purpose(key_id, key_purpose, read_protect)?;
        }

        Ok(())
    }

    fn key_purpose(key_id: u8) -> KeyPurpose {
        let efuse_field = match key_id {
            0 => hal_efuse::KEY_PURPOSE_0,
            1 => hal_efuse::KEY_PURPOSE_1,
            2 => hal_efuse::KEY_PURPOSE_2,
            3 => hal_efuse::KEY_PURPOSE_3,
            4 => hal_efuse::KEY_PURPOSE_4,
            5 => hal_efuse::KEY_PURPOSE_5,
            _ => panic!("invalid efuse integer"),
        };
        let field_value: u8 = Efuse::read_field_le(efuse_field);
        KeyPurpose::try_from(field_value).expect("key purpose was invalid")
    }

    pub fn is_key_written(key_id: u8) -> bool {
        Self::key_purpose(key_id) != KeyPurpose::User
    }

    pub fn read_efuse(&self, key_id: u8) -> Result<[u8; 32], EfuseError> {
        let field = match key_id {
            0 => hal_efuse::KEY0,
            1 => hal_efuse::KEY1,
            2 => hal_efuse::KEY2,
            3 => hal_efuse::KEY3,
            4 => hal_efuse::KEY4,
            5 => hal_efuse::KEY5,
            _ => panic!("invalid efuse integer"),
        };
        let bytes: [u8; 32] = Efuse::read_field_le::<[u8; 32]>(field);

        Ok(bytes)
    }

    /// # Safety
    unsafe fn write_key_purpose(
        &self,
        key_number: u8,
        key_purpose: KeyPurpose,
        read_protect: bool,
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

        // We bundle every config in Block 0 to minimize write operations
        // Todo: Write key purpose and rw flags for multiple keys
        // Disable write to key block
        let mut write_disable: u32 = 0x01 << (WR_DIS_KEY_OFFSET + key_number);
        // Disable write to key purpose
        write_disable += 0x01 << (WR_DIS_KP_OFFSET + key_number);
        buff[0..4].copy_from_slice(&write_disable.to_le_bytes());

        if read_protect {
            self.set_read_protect(key_number, &mut buff);
        }

        self.write_block(&buff, 0)
    }

    /// # Safety
    unsafe fn set_read_protect(&self, key_number: u8, buff: &mut [u8; 32]) {
        // Disable read key
        let read_disable = 0x01_u8 << key_number;
        buff[4] = read_disable;
    }

    /// # Safety
    unsafe fn write_block(&self, data: &[u8; 32], block_number: u8) -> Result<(), EfuseError> {
        let efuse = &self.efuse;

        let mut to_burn: [u32; 11] = [0; 11];

        // Generate and write Reed-Solomon ECC
        // Efuse controller ignores RS code for blocks 0 and 1
        let rs_enc = reed_solomon::Encoder::new(12);
        let ecc = rs_enc.encode(data);

        // Flip efuse words to little endian
        for (i, word) in ecc.chunks(4).enumerate() {
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
        efuse.conf().write(|w| w.op_code().bits(WRITE_COMMAND));

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
        efuse.conf().write(|w| w.op_code().bits(READ_COMMAND));
        efuse.cmd().write(|w| w.read_cmd().set_bit());

        // Poll command register until read bit is cleared
        while efuse.cmd().read().read_cmd().bit_is_set() {}
    }
}

pub struct EfuseHmacKeys<'a> {
    pub share_encryption: EfuseHmacKey<'a>,
    pub fixed_entropy: EfuseHmacKey<'a>,
}

impl<'a> EfuseHmacKeys<'a> {
    const ENCRYPTION_KEYID: esp_hal::hmac::KeyId = esp_hal::hmac::KeyId::Key0;
    const FIXED_ENTROPY_KEYID: esp_hal::hmac::KeyId = esp_hal::hmac::KeyId::Key1;
    const HMAC_KEYIDS: [esp_hal::hmac::KeyId; 2] =
        [Self::ENCRYPTION_KEYID, Self::FIXED_ENTROPY_KEYID];

    pub fn has_been_initialized() -> bool {
        for key_id in Self::HMAC_KEYIDS {
            if !EfuseController::is_key_written(key_id as u8) {
                return false;
            }
        }

        true
    }
    pub fn load_or_init(
        efuse: &EfuseController,
        hmac: &'a core::cell::RefCell<esp_hal::hmac::Hmac<'a>>,
        read_protect: bool,
        rng: &mut impl RngCore,
    ) -> Result<Self, EfuseError> {
        for key_id in Self::HMAC_KEYIDS {
            if let Some(written_key) = efuse.randomly_init_key(
                key_id as u8,
                KeyPurpose::HmacUpstream,
                read_protect,
                rng,
            )? {
                use hmac::Mac as _;
                let mut tmp_hmac = EfuseHmacKey::new(hmac, key_id);
                let domain_sep = "factory-test";
                let input = [42u8; 33];
                let output = tmp_hmac.hash(domain_sep, input.as_slice()).unwrap();
                type HmacSha256 = hmac::Hmac<sha2::Sha256>;
                let mut mac = HmacSha256::new_from_slice(&written_key[..])
                    .expect("HMAC can take key of any size");
                mac.update(&[domain_sep.len() as u8]);
                mac.update(domain_sep.as_bytes());
                mac.update(input.as_slice());
                let expected_output = mac.finalize().into_bytes();
                assert_eq!(expected_output, output.into());
            }
        }

        assert!(Self::has_been_initialized());

        Ok(EfuseHmacKeys {
            share_encryption: EfuseHmacKey::new(hmac, Self::ENCRYPTION_KEYID),
            fixed_entropy: EfuseHmacKey::new(hmac, Self::FIXED_ENTROPY_KEYID),
        })
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
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

impl TryFrom<u8> for KeyPurpose {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(KeyPurpose::User),
            4 => Ok(KeyPurpose::Aes128),
            5 => Ok(KeyPurpose::HmacDownstream),
            6 => Ok(KeyPurpose::JtagHmacDownstream),
            7 => Ok(KeyPurpose::Ds),
            8 => Ok(KeyPurpose::HmacUpstream),
            9 => Ok(KeyPurpose::SecureBootDigest0),
            10 => Ok(KeyPurpose::SecureBootDigest1),
            11 => Ok(KeyPurpose::SecureBootDigest2),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub enum EfuseError {
    EfuseReadError,
    EfuseWriteError(u8),
    EfuseAlreadyBurned,
    EfuseError,
}

pub struct EfuseHmacKey<'a> {
    hmac: &'a core::cell::RefCell<esp_hal::hmac::Hmac<'a>>,
    hmac_key_id: esp_hal::hmac::KeyId,
}

impl<'a> EfuseHmacKey<'a> {
    pub fn new(
        hmac: &'a core::cell::RefCell<esp_hal::hmac::Hmac<'a>>,
        hmac_key_id: esp_hal::hmac::KeyId,
    ) -> Self {
        Self { hmac, hmac_key_id }
    }

    pub fn hash(
        &mut self,
        domain_separator: &str,
        input: &[u8],
    ) -> Result<[u8; 32], esp_hal::hmac::Error> {
        let mut hmac = self.hmac.borrow_mut();
        let mut output = [0u8; 32];
        let mut remaining = input;

        hmac.init();
        nb::block!(hmac.configure(esp_hal::hmac::HmacPurpose::ToUser, self.hmac_key_id))?;

        let len_byte = [domain_separator.len() as u8];
        let _its_one_byte = nb::block!(hmac.update(&len_byte[..])).unwrap();
        let mut ds_remaining = domain_separator.as_bytes();

        while !ds_remaining.is_empty() {
            ds_remaining = nb::block!(hmac.update(ds_remaining)).unwrap();
        }

        while !remaining.is_empty() {
            remaining = nb::block!(hmac.update(remaining)).unwrap();
        }

        nb::block!(hmac.finalize(output.as_mut_slice())).unwrap();

        Ok(output)
    }

    pub fn mix_in_rng(&mut self, rng: &mut impl RngCore) -> impl RngCore {
        let mut entropy = [0u8; 64];
        rng.fill_bytes(&mut entropy);
        let chacha_seed = self.hash("mix-in-rng", &entropy).expect("entropy hash");
        rand_chacha::ChaCha20Rng::from_seed(chacha_seed)
    }
}

impl frostsnap_core::device::DeviceSecretDerivation for EfuseHmacKey<'_> {
    fn get_share_encryption_key(
        &mut self,
        access_structure_ref: AccessStructureRef,
        party_index: frostsnap_core::schnorr_fun::frost::ShareIndex,
        coord_key: frostsnap_core::CoordShareDecryptionContrib,
    ) -> frostsnap_core::SymmetricKey {
        let mut src = [0u8; 128];
        src[..32].copy_from_slice(access_structure_ref.key_id.to_bytes().as_slice());
        src[32..64].copy_from_slice(
            access_structure_ref
                .access_structure_id
                .to_bytes()
                .as_slice(),
        );
        src[64..96].copy_from_slice(party_index.to_bytes().as_slice());
        src[96..128].copy_from_slice(coord_key.to_bytes().as_slice());

        let output = self.hash("share-encryption", &src).unwrap();

        frostsnap_core::SymmetricKey(output)
    }

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: frostsnap_core::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32] {
        let mut input = [0u8; 52]; // 16 (stream_id) + 4 (index) + 32 (seed_material)
        input[..16].copy_from_slice(nonce_stream_id.to_bytes().as_slice());
        input[16..20].copy_from_slice(&index.to_be_bytes());
        input[20..52].copy_from_slice(seed_material);

        self.hash("nonce-seed", &input).unwrap()
    }
}
