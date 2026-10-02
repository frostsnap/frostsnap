use crate::flash::FactoryDataHandle;
use alloc::collections::VecDeque;
use esp_hal::{peripherals::DS, sha::Sha};
use frostsnap_comms::factory::DS_KEY_SIZE_BITS;
use frostsnap_comms::factory::{pad_message_for_rsa, PaddedMessageBlock};
use frostsnap_comms::genuine_certificate::DsSignature;
use nb::block;

/// What a DS signature is for, so the device loop can build the reply once it completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignRequest {
    FactoryAttestation(frostsnap_comms::GenuineChallenge),
}

enum State {
    Idle,
    Computing(SignRequest),
    /// `set_finish` has been issued; the peripheral is busy until it clears.
    Finishing,
}

/// Signs with the ESP32 Digital Signature peripheral without blocking the device loop: a
/// signature takes around 700 ms, during which the loop must keep serving USB and the chain.
pub struct HardwareDs<'a> {
    ds: DS<'a>,
    factory_data: FactoryDataHandle<'a>,
    queue: VecDeque<(SignRequest, PaddedMessageBlock)>,
    state: State,
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
    pub fn new(ds: DS<'a>, factory_data: FactoryDataHandle<'a>) -> Self {
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
            factory_data,
            queue: VecDeque::new(),
            state: State::Idle,
        }
    }

    pub fn factory_data(&self) -> FactoryDataHandle<'a> {
        self.factory_data
    }

    /// Queues a signature over `message`. The result comes out of [`Self::poll`].
    ///
    /// Pending work stays bounded whatever the host sends: a request identical to the one computing
    /// or one queued is dropped, and a new request replaces any queued request of the same kind.
    /// The coordinator waits on only its newest request, so a replaced one would go unanswered
    /// anyway.
    pub fn push(&mut self, request: SignRequest, message: &[u8], sha256: &mut Sha<'_>) {
        let in_flight = matches!(self.state, State::Computing(computing) if computing == request);
        if in_flight || self.queue.iter().any(|(queued, _)| *queued == request) {
            return;
        }
        self.queue.retain(|(queued, _)| {
            core::mem::discriminant(queued) != core::mem::discriminant(&request)
        });
        let mut digest = [0u8; 32];
        let mut hasher = sha256.start::<esp_hal::sha::Sha256>();
        let mut remaining = message;
        while !remaining.is_empty() {
            remaining = block!(hasher.update(remaining)).expect("infallible");
        }
        block!(hasher.finish(&mut digest)).unwrap();
        self.queue
            .push_back((request, pad_message_for_rsa(&digest)));
    }

    /// Call on every iteration of the device loop. Never waits on the exponentiation.
    pub fn poll(&mut self) -> Option<(SignRequest, DsSignature)> {
        let regs = self.ds.register_block();
        match self.state {
            State::Computing(request) => {
                if regs.query_busy().read().query_busy().bit() {
                    return None;
                }
                let signature = words_to_signature(&read_signature(&self.ds));
                regs.set_finish().write(|w| w.set_finish().set_bit());
                self.state = State::Finishing;
                return Some((request, signature));
            }
            State::Finishing => {
                if regs.query_busy().read().query_busy().bit() {
                    return None;
                }
                self.state = State::Idle;
            }
            State::Idle => {}
        }
        let (request, padded_message) = self.queue.pop_front()?;
        let factory_data = self.factory_data.read().ok()?;
        start_exponentiation(&self.ds, &factory_data.ds_encrypted_params, padded_message);
        self.state = State::Computing(request);
        None
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
fn start_exponentiation(ds: &DS<'_>, encrypted_params: &[u8], block: PaddedMessageBlock) {
    let mut challenge = block.0;
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
