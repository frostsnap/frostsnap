//! The device's [`CryptoHardware`]: esp-hal's SHA, HMAC and RSA drivers, and the DS peripheral,
//! which esp-hal has no driver for.

use crate::flash::FactoryDataHandle;
use crate::partitions::{EspFlashPartition, PartitionExt};
use crate::secure_boot::{self, SecureBootError};
use esp_hal::{
    hmac::{Hmac, HmacPurpose, KeyId},
    peripherals::{DS, HMAC, RSA},
    rsa::Rsa,
    sha::{Sha, Sha256},
    Blocking,
};
use frostsnap_comms::factory::{PaddedMessageBlock, DS_KEY_SIZE_BITS};
use frostsnap_comms::genuine_certificate::DsSignature;
use frostsnap_comms::Sha256Digest;
use frostsnap_embedded::crypto_engine::{CryptoHardware, FactoryDataUnreadable};
use nb::block;

pub use frostsnap_embedded::crypto_engine::{Attestation, HmacKeyIds, SignRequest};

pub type CryptoEngine<'d> = frostsnap_embedded::crypto_engine::CryptoEngine<EspCrypto<'d>>;
pub type Idle<'e, 'd> = frostsnap_embedded::crypto_engine::Idle<'e, EspCrypto<'d>>;
pub type FirmwareChecker<'e, 'd> =
    frostsnap_embedded::crypto_engine::FirmwareChecker<'e, EspCrypto<'d>>;
pub type HmacKey<'h, 'd> = frostsnap_embedded::crypto_engine::HmacKey<'h, EspCrypto<'d>>;

pub struct EspCrypto<'d> {
    sha: Sha<'d>,
    hmac: Hmac<'d>,
    rsa: Rsa<'d, Blocking>,
    ds: DS<'d>,
    factory_data: Option<FactoryDataHandle<'d>>,
}

impl<'d> EspCrypto<'d> {
    /// Without factory data there is no DS key, and the DS is left alone.
    pub fn new(
        sha: Sha<'d>,
        hmac: Hmac<'d>,
        rsa: RSA<'d>,
        ds: DS<'d>,
        factory_data: Option<FactoryDataHandle<'d>>,
    ) -> Self {
        if factory_data.is_some() {
            enable_ds();
        }
        // The DS reset also resets the MPI block, so the RSA driver is made after it.
        let rsa = Rsa::new(rsa);
        Self {
            sha,
            hmac,
            rsa,
            ds,
            factory_data,
        }
    }
}

/// No esp-hal driver enables the DS clock (none references `Peripheral::Ds`), so this does what
/// `Sha::new` and `Hmac::new` do through `GenericPeripheralGuard`.
///
/// `perip_clk_en1` and `perip_rst_en1` are shared by every crypto peripheral, so the
/// read-modify-write runs in a critical section, as esp-hal's `PeripheralClockControl` does.
fn enable_ds() {
    critical_section::with(|_| {
        let sys = esp_hal::peripherals::SYSTEM::regs();
        sys.perip_clk_en1()
            .modify(|_, w| w.crypto_ds_clk_en().set_bit());
        sys.perip_rst_en1()
            .modify(|_, w| w.crypto_ds_rst().set_bit());
        sys.perip_rst_en1()
            .modify(|_, w| w.crypto_ds_rst().clear_bit());
    });
}

impl<'d> CryptoHardware for EspCrypto<'d> {
    type HmacKeyId = KeyId;
    type FactoryData = FactoryDataHandle<'d>;
    type FirmwareImage = EspFlashPartition<'d>;
    type SecureBootError = SecureBootError<'static>;

    fn factory_data(&self) -> Option<FactoryDataHandle<'d>> {
        self.factory_data
    }

    fn sha256(&mut self, message: &[u8]) -> [u8; 32] {
        let mut digest = [0u8; 32];
        let mut hasher = self.sha.start::<Sha256>();
        let mut remaining = message;
        while !remaining.is_empty() {
            remaining = block!(hasher.update(remaining)).expect("infallible");
        }
        block!(hasher.finish(&mut digest)).expect("infallible");
        digest
    }

    fn hmac(&mut self, key: KeyId, domain: &str, input: &[u8]) -> [u8; 32] {
        let hmac = &mut self.hmac;
        let mut output = [0u8; 32];
        hmac.init();
        block!(hmac.configure(HmacPurpose::ToUser, key)).expect("eFuse key purpose is HMAC");

        let len_byte = [domain.len() as u8];
        for part in [&len_byte[..], domain.as_bytes(), input] {
            let mut rest = part;
            while !rest.is_empty() {
                rest = block!(hmac.update(rest)).expect("infallible");
            }
        }
        block!(hmac.finalize(output.as_mut_slice())).expect("infallible");
        output
    }

    fn ds_start(&mut self, block: &PaddedMessageBlock) -> Result<(), FactoryDataUnreadable> {
        let factory_data = self
            .factory_data
            .ok_or(FactoryDataUnreadable)?
            .read()
            .map_err(|_| {
                log!("DS parameters unreadable; attestation dropped");
                FactoryDataUnreadable
            })?;
        start_exponentiation(&self.ds, &factory_data.ds_encrypted_params, block);
        Ok(())
    }

    fn ds_busy(&self) -> bool {
        self.ds
            .register_block()
            .query_busy()
            .read()
            .query_busy()
            .bit()
    }

    fn ds_read_signature(&mut self) -> DsSignature {
        words_to_signature(&read_signature(&self.ds))
    }

    fn hmac_invalidate_ds_key(&mut self) {
        // ESP-IDF's hmac_ll_clean, in its order. esp-hal's Hmac has no clean-up of its own.
        let regs = HMAC::regs();
        regs.set_invalidate_ds()
            .write(|w| w.set_invalidate_ds().set_bit());
        regs.set_invalidate_jtag()
            .write(|w| w.set_invalidate_jtag().set_bit());
    }

    fn ds_set_finish(&mut self) {
        self.ds
            .register_block()
            .set_finish()
            .write(|w| w.set_finish().set_bit());
    }

    fn firmware_digest(
        &mut self,
        image: &EspFlashPartition<'d>,
        up_to: Option<u32>,
    ) -> Sha256Digest {
        image.sha256_digest(&mut self.sha, up_to)
    }

    fn verify_secure_boot(
        &mut self,
        image: &EspFlashPartition<'d>,
    ) -> Result<(), SecureBootError<'static>> {
        secure_boot::verify_secure_boot(image, &mut self.rsa, &mut self.sha)
    }
}

fn words_to_signature(words: &[u32; 96]) -> DsSignature {
    let mut result = [0u8; 384];
    for (i, &word) in words.iter().rev().enumerate() {
        let bytes = word.to_be_bytes();
        let start = i * 4;
        result[start..start + 4].copy_from_slice(&bytes);
    }
    DsSignature(result)
}

/// Waits only for the key check that `set_start` begins, which is short; the exponentiation runs
/// on after this returns.
fn start_exponentiation(ds: &DS<'_>, encrypted_params: &[u8], block: &PaddedMessageBlock) {
    let mut challenge = block.0;
    challenge.reverse();

    let iv = &encrypted_params[..16];
    let ciph = &encrypted_params[16..];
    if ciph.len() != 1200 {
        panic!("incorrect cipher length!");
    }
    let y_ciph = &ciph[0..384];
    let m_ciph = &ciph[384..768];
    let rb_ciph = &ciph[768..1152];
    let box_ciph = &ciph[1152..1200];

    let regs = ds.register_block();

    regs.set_start().write(|w| w.set_start().set_bit());
    while regs.query_busy().read().query_busy().bit() {}
    if regs.query_key_wrong().read().query_key_wrong().bits() != 0 {
        panic!("DS key read error!");
    }

    for (i, v) in iv.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        regs.iv_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in challenge.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        regs.x_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in y_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        regs.y_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in m_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        regs.m_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in rb_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        regs.rb_mem(i).write(|w| unsafe { w.bits(data) });
    }

    for (i, v) in box_ciph.chunks(4).enumerate() {
        let data = u32::from_le_bytes(v.try_into().unwrap());
        regs.box_mem(i).write(|w| unsafe { w.bits(data) });
    }

    regs.set_continue().write(|w| w.set_continue().set_bit());
}

fn read_signature(ds: &DS<'_>) -> [u32; 96] {
    let regs = ds.register_block();

    let mut sig = [0u32; 96];
    if regs.query_check().read().bits() == 0 {
        for (i, sig_word) in sig.iter_mut().enumerate().take(DS_KEY_SIZE_BITS / 32) {
            let word = regs.z_mem(i).read().bits();
            *sig_word = word;
        }
    } else {
        panic!("Failed to read signature from DS!")
    }

    sig
}
