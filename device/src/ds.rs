use alloc::vec::Vec;
use esp_hal::{peripherals::DS, sha::Sha};
use frostsnap_comms::factory::pad_message_for_rsa;
use frostsnap_comms::factory::DS_KEY_SIZE_BITS;
use nb::block;

/// Hardware DS signing implementation using ESP32's Digital Signature peripheral.
pub struct HardwareDs<'a> {
    ds: DS<'a>,
    encrypted_params: Vec<u8>,
}

impl<'a> HardwareDs<'a> {
    /// Create a new HardwareDs instance and enable the DS peripheral clock.
    ///
    /// In esp-hal v1.0.0 no driver enables the DS clock for us (no internal
    /// driver references `Peripheral::Ds`), so we do it ourselves via the
    /// public PAC. This mirrors what `Sha::new` and `Hmac::new` do internally
    /// via `GenericPeripheralGuard`.
    ///
    /// The `perip_clk_en1` / `perip_rst_en1` registers are shared bit-fields
    /// for every crypto peripheral (DS, HMAC, SHA, AES, RSA, ECC, ...), so
    /// the read-modify-write must run inside a critical section to avoid
    /// clobbering a concurrent update from an interrupt handler. esp-hal's
    /// own `PeripheralClockControl` does the same via its `NonReentrantMutex`.
    pub fn new(ds: DS<'a>, encrypted_params: Vec<u8>) -> Self {
        critical_section::with(|_| {
            let sys = esp_hal::peripherals::SYSTEM::regs();
            sys.perip_clk_en1()
                .modify(|_, w| w.crypto_ds_clk_en().set_bit());
            sys.perip_rst_en1()
                .modify(|_, w| w.crypto_ds_rst().set_bit());
            sys.perip_rst_en1()
                .modify(|_, w| w.crypto_ds_rst().clear_bit());
        });

        Self {
            ds,
            encrypted_params,
        }
    }

    /// Sign a message using the hardware DS peripheral
    pub fn sign(&mut self, message: &[u8], sha256: &mut Sha<'_>) -> [u8; 384] {
        // Calculate message digest using hardware SHA and apply padding
        let mut digest = [0u8; 32];
        let mut hasher = sha256.start::<esp_hal::sha::Sha256>();
        let mut remaining = message;
        while !remaining.is_empty() {
            remaining = block!(hasher.update(remaining)).expect("infallible");
        }
        block!(hasher.finish(&mut digest)).unwrap();

        let padded_message = pad_message_for_rsa(&digest);
        let sig = private_exponentiation(&self.ds, &self.encrypted_params, padded_message);
        words_to_bytes(&sig)
    }
}

fn words_to_bytes(words: &[u32; 96]) -> [u8; 384] {
    let mut result = [0u8; 384];
    for (i, &word) in words.iter().rev().enumerate() {
        let bytes = word.to_be_bytes();
        let start = i * 4;
        result[start..start + 4].copy_from_slice(&bytes);
    }
    result
}

fn private_exponentiation(
    ds: &DS<'_>,
    encrypted_params: &[u8],
    mut challenge: [u8; 384],
) -> [u32; 96] {
    challenge.reverse();

    let iv = &encrypted_params[..16];
    let ciph = &encrypted_params[16..];
    let y_ciph = &ciph[0..384];
    let m_ciph = &ciph[384..768];
    let rb_ciph = &ciph[768..1152];
    let box_ciph = &ciph[1152..1200];
    if ciph.len() != 1200 {
        panic!("incorrect cipher length!");
    }

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
    while regs.query_busy().read().query_busy().bit_is_set() {}

    let mut sig = [0u32; 96];
    if regs.query_check().read().bits() == 0 {
        for (i, sig_word) in sig.iter_mut().enumerate().take(DS_KEY_SIZE_BITS / 32) {
            let word = regs.z_mem(i).read().bits();
            *sig_word = word;
        }
    } else {
        panic!("Failed to read signature from DS!")
    }

    regs.set_finish().write(|w| w.set_finish().set_bit());
    while regs.query_busy().read().query_busy().bit() {}

    sig
}
