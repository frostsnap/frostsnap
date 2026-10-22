//! The device's crypto capabilities over esp-hal's SHA, HMAC and RSA drivers, and the DS
//! peripheral, which esp-hal has no driver for.
//!
//! For the whole of a signature the DS drives the HMAC peripheral (which runs on SHA) and the MPI
//! block RSA uses. Nothing else may touch SHA, HMAC or RSA from the start of a signature until its
//! finish and clean-up are complete. [`EspCrypto`] owns the peripherals and keeps that rule by
//! lending them only through [`NormalMode`], which can't exist while a signature is in flight.

use crate::flash::FactoryDataHandle;
use crate::partitions::{EspFlashPartition, PartitionExt};
use crate::secure_boot::{self, SecureBootError};
use alloc::boxed::Box;
use esp_hal::time::Instant;
use esp_hal::{
    hmac::{Hmac, HmacPurpose, KeyId},
    peripherals::{DS, HMAC, RSA},
    rsa::Rsa,
    sha::Sha,
    Blocking,
};
use frostsnap_comms::factory::{self, PaddedMessageBlock, DS_KEY_SIZE_BITS};
use frostsnap_comms::genuine_certificate::{Certificate, DsSignature};
use frostsnap_comms::genuine_check::AttestationSignature;
use frostsnap_comms::Sha256Digest;
use frostsnap_core::device::DevicePrf;
use nb::block;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttestError {
    NoFactoryKey,
    FactoryDataUnreadable,
    /// The signer refused the key or its own result.
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestPoll {
    Normal,
    InFlight,
    /// The outcome of the last [`EspCrypto::start_attestation`], returned once.
    Done(Result<Box<AttestationSignature>, AttestError>),
}

/// ESP-IDF's `SOC_DS_KEY_CHECK_MAX_WAIT_US` for the C3: a DS still busy checking its key after this
/// got no valid key from the HMAC. It is the only DS wait ESP-IDF bounds.
const DS_KEY_CHECK_MAX_WAIT_US: u64 = 1100;

#[derive(Debug, Clone, Copy)]
pub struct HmacKeyIds {
    pub share_encryption: KeyId,
    pub fixed_entropy: KeyId,
}

/// The DS key's encrypted parameters, and the eFuse key the HMAC decrypts them with.
#[derive(Clone, Copy)]
pub struct DsKey<'d> {
    pub factory_data: FactoryDataHandle<'d>,
    pub efuse_key: KeyId,
}

pub struct EspCrypto<'d> {
    sha: Sha<'d>,
    hmac: Hmac<'d>,
    rsa: Rsa<'d, Blocking>,
    ds: DS<'d>,
    ds_key: Option<DsKey<'d>>,
    keys: HmacKeyIds,
    state: State,
}

enum State {
    Normal,
    Computing,
    Finishing {
        outcome: Result<Box<AttestationSignature>, AttestError>,
    },
}

impl<'d> EspCrypto<'d> {
    /// Without factory data there is no DS key, and the DS is left alone.
    pub fn new(
        sha: Sha<'d>,
        hmac: Hmac<'d>,
        rsa: RSA<'d>,
        ds: DS<'d>,
        ds_key: Option<DsKey<'d>>,
        keys: HmacKeyIds,
    ) -> Self {
        if ds_key.is_some() {
            enable_ds();
        }
        // The DS reset also resets the MPI block, so the RSA driver is made after it.
        let rsa = Rsa::new(rsa);
        Self {
            sha,
            hmac,
            rsa,
            ds,
            ds_key,
            keys,
            state: State::Normal,
        }
    }

    /// Returns the DS, and the MPI block it shares with RSA, to idle, abandoning a signature. The
    /// MPI then clears its memory, which [`Self::wait_mpi_ready`] waits for.
    fn abort(&mut self) {
        reset_ds();
        self.hmac_invalidate_ds_key();
    }

    /// ESP-IDF's `mpi_hal_enable_hardware_hw_op` waits for this without a bound.
    fn wait_mpi_ready(&self) {
        while !RSA::regs().query_clean().read().query_clean().bit_is_set() {}
    }

    fn ds_busy(&self) -> bool {
        self.ds
            .register_block()
            .query_busy()
            .read()
            .query_busy()
            .bit()
    }

    /// ESP-IDF's `hmac_hal_clean`: wait for the HMAC to go idle, then `hmac_ll_clean`'s writes,
    /// `SET_INVALIDATE_DS` then `SET_INVALIDATE_JTAG`. esp-hal's Hmac has no clean-up of its own.
    fn hmac_invalidate_ds_key(&mut self) {
        wait_hmac_idle();
        let regs = HMAC::regs();
        regs.set_invalidate_ds()
            .write(|w| w.set_invalidate_ds().set_bit());
        regs.set_invalidate_jtag()
            .write(|w| w.set_invalidate_jtag().set_bit());
    }

    /// Reads the DS parameters, configures the HMAC for the DS key and starts the exponentiation
    /// of `block`. On an error the DS is left idle.
    fn ds_start(&mut self, block: &PaddedMessageBlock) -> Result<(), AttestError> {
        let ds_key = self.ds_key.ok_or(AttestError::NoFactoryKey)?;
        let factory_data = ds_key.factory_data.read().map_err(|_| {
            log!("DS parameters unreadable; attestation dropped");
            AttestError::FactoryDataUnreadable
        })?;
        // The DS takes its key from the HMAC, which must be configured for the DS eFuse key first,
        // as ESP-IDF's esp_ds_start_sign does. Without this the DS failed its parameter check
        // after a reset.
        configure_hmac(&mut self.hmac, HmacPurpose::ToDs, ds_key.efuse_key)
            .expect("eFuse key purpose is DS");
        let started = start_exponentiation(&self.ds, &factory_data.ds_encrypted_params, block);
        if started.is_err() {
            self.abort();
        }
        started
    }
}

impl<'d> EspCrypto<'d> {
    /// `None` from the start of a signature until its finish and clean-up are complete. Never
    /// waits.
    pub fn try_normal_mode(&mut self) -> Option<NormalMode<'_, 'd>> {
        match self.state {
            State::Normal => Some(NormalMode { crypto: self }),
            State::Computing | State::Finishing { .. } => None,
        }
    }

    /// Abandons any signature in flight, whose outcome is then never returned.
    pub fn force_normal_mode(&mut self) -> NormalMode<'_, 'd> {
        if !matches!(
            core::mem::replace(&mut self.state, State::Normal),
            State::Normal
        ) {
            self.abort();
        }
        NormalMode { crypto: self }
    }
}

/// The crypto peripherals while no signature is in flight. Lends the HMAC under the user keys
/// ([`Self::share_encryption`], [`Self::fixed_entropy`]), the SHA ([`Self::digest`]) and the RSA
/// ([`Self::verify_secure_boot`]).
pub struct NormalMode<'a, 'd> {
    crypto: &'a mut EspCrypto<'d>,
}

impl<'d> NormalMode<'_, 'd> {
    pub fn share_encryption(&mut self) -> HmacPrf<'_, 'd> {
        let key = self.crypto.keys.share_encryption;
        HmacPrf {
            crypto: self.crypto,
            key,
        }
    }

    pub fn fixed_entropy(&mut self) -> HmacPrf<'_, 'd> {
        let key = self.crypto.keys.fixed_entropy;
        HmacPrf {
            crypto: self.crypto,
            key,
        }
    }
}

/// The HMAC peripheral under one eFuse key, which software can't read. Every secret the device
/// derives depends on its output staying the same for the device's whole life.
pub struct HmacPrf<'a, 'd> {
    crypto: &'a mut EspCrypto<'d>,
    key: KeyId,
}

impl DevicePrf for HmacPrf<'_, '_> {
    fn prf(&mut self, message: &[&[u8]]) -> [u8; 32] {
        let hmac = &mut self.crypto.hmac;
        let mut output = [0u8; 32];
        configure_hmac(hmac, HmacPurpose::ToUser, self.key).expect("eFuse key purpose is HMAC");
        for part in message {
            let mut rest = *part;
            while !rest.is_empty() {
                rest = block!(hmac.update(rest)).expect("infallible");
            }
        }
        block!(hmac.finalize(output.as_mut_slice())).expect("infallible");
        output
    }
}

impl EspCrypto<'_> {
    /// Read from the factory data on each call; `None` on a device without a certificate, or when
    /// the factory data can't be read.
    pub fn certificate(&self) -> Option<Certificate> {
        Some(self.ds_key?.factory_data.read().ok()?.certificate)
    }

    /// Abandons any attestation in flight, whose outcome is then never returned, and starts a
    /// signature over `digest`.
    pub fn start_attestation(&mut self, digest: &Sha256Digest) -> Result<(), AttestError> {
        self.force_normal_mode();
        self.wait_mpi_ready();
        self.ds_start(&factory::pad_message_for_rsa(&digest.0))?;
        self.state = State::Computing;
        Ok(())
    }

    /// Advances a signature in flight by one step. Never waits.
    pub fn poll(&mut self) -> AttestPoll {
        self.state = match core::mem::replace(&mut self.state, State::Normal) {
            State::Normal => return AttestPoll::Normal,
            State::Computing if self.ds_busy() => State::Computing,
            State::Computing => {
                let outcome = read_signature(&self.ds).map(|words| {
                    Box::new(AttestationSignature::Rsa3072(words_to_signature(&words)))
                });
                // ESP-IDF's esp_ds_finish_sign order: hmac_hal_clean, then ds_hal_finish.
                self.hmac_invalidate_ds_key();
                self.ds
                    .register_block()
                    .set_finish()
                    .write(|w| w.set_finish().set_bit());
                State::Finishing { outcome }
            }
            State::Finishing { outcome } if self.ds_busy() => State::Finishing { outcome },
            State::Finishing { outcome } => return AttestPoll::Done(outcome),
        };
        AttestPoll::InFlight
    }
}

impl<'d> NormalMode<'_, 'd> {
    /// SHA-256 of a firmware image, up to `up_to` bytes.
    pub fn digest(&mut self, image: &EspFlashPartition<'d>, up_to: Option<u32>) -> Sha256Digest {
        image.sha256_digest(&mut self.crypto.sha, up_to)
    }

    /// The secure-boot check of a firmware image against the eFuse key digest.
    pub fn verify_secure_boot(
        &mut self,
        image: &EspFlashPartition<'d>,
    ) -> Result<(), SecureBootError<'static>> {
        self.crypto.wait_mpi_ready();
        secure_boot::verify_secure_boot(image, &mut self.crypto.rsa, &mut self.crypto.sha)
    }
}

/// No esp-hal driver enables the DS clock (none references `Peripheral::Ds`), so this does what
/// `Sha::new` and `Hmac::new` do through `GenericPeripheralGuard`.
///
/// `perip_clk_en1` and `perip_rst_en1` are shared by every crypto peripheral, so the
/// read-modify-write runs in a critical section, as esp-hal's `PeripheralClockControl` does.
fn enable_ds() {
    critical_section::with(|_| {
        esp_hal::peripherals::SYSTEM::regs()
            .perip_clk_en1()
            .modify(|_, w| w.crypto_ds_clk_en().set_bit());
    });
    reset_ds();
}

fn reset_ds() {
    critical_section::with(|_| {
        let sys = esp_hal::peripherals::SYSTEM::regs();
        sys.perip_rst_en1()
            .modify(|_, w| w.crypto_ds_rst().set_bit());
        sys.perip_rst_en1()
            .modify(|_, w| w.crypto_ds_rst().clear_bit());
    });
}

/// ESP-IDF's `hmac_hal_start` + `hmac_hal_configure` order. esp-hal 1.2.2's `Hmac::configure` reads
/// the error flag without waiting for the HMAC to go idle, and was seen to report a spurious key
/// purpose mismatch in some firmware layouts.
fn configure_hmac(
    hmac: &mut Hmac<'_>,
    purpose: HmacPurpose,
    key: KeyId,
) -> Result<(), esp_hal::hmac::Error> {
    let regs = HMAC::regs();
    wait_hmac_idle();
    hmac.init();
    wait_hmac_idle();
    regs.set_para_purpose()
        .write(|w| unsafe { w.purpose_set().bits(purpose as u8) });
    regs.set_para_key()
        .write(|w| unsafe { w.key_set().bits(key as u8) });
    regs.set_para_finish().write(|w| w.set_para_end().set_bit());
    wait_hmac_idle();
    if regs.query_error().read().query_check().bit_is_set() {
        regs.set_result_finish()
            .write(|w| w.set_result_end().set_bit());
        return Err(esp_hal::hmac::Error::KeyPurposeMismatch);
    }
    wait_hmac_idle();
    Ok(())
}

fn wait_hmac_idle() {
    while HMAC::regs().query_busy().read().busy_state().bit_is_set() {}
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
fn start_exponentiation(
    ds: &DS<'_>,
    encrypted_params: &[u8],
    block: &PaddedMessageBlock,
) -> Result<(), AttestError> {
    let mut challenge = block.0;
    challenge.reverse();

    let Some((iv, ciph)) = encrypted_params
        .split_at_checked(16)
        .filter(|(_, ciph)| ciph.len() == 1200)
    else {
        log!("DS parameters have the wrong length; attestation dropped");
        return Err(AttestError::Rejected);
    };
    let y_ciph = &ciph[0..384];
    let m_ciph = &ciph[384..768];
    let rb_ciph = &ciph[768..1152];
    let box_ciph = &ciph[1152..1200];

    let regs = ds.register_block();

    regs.set_start().write(|w| w.set_start().set_bit());
    let started = Instant::now();
    while regs.query_busy().read().query_busy().bit() {
        if started.elapsed().as_micros() > DS_KEY_CHECK_MAX_WAIT_US {
            log!("DS key check overran; attestation dropped");
            return Err(AttestError::Rejected);
        }
    }
    if regs.query_key_wrong().read().query_key_wrong().bits() != 0 {
        log!("DS got no key from the HMAC; attestation dropped");
        return Err(AttestError::Rejected);
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
    Ok(())
}

fn read_signature(ds: &DS<'_>) -> Result<[u32; 96], AttestError> {
    let regs = ds.register_block();
    let check = regs.query_check().read().bits();
    if check != 0 {
        log!("DS rejected its own result (check {check}); attestation dropped");
        return Err(AttestError::Rejected);
    }
    let mut sig = [0u32; 96];
    for (i, sig_word) in sig.iter_mut().enumerate().take(DS_KEY_SIZE_BITS / 32) {
        *sig_word = regs.z_mem(i).read().bits();
    }
    Ok(sig)
}
