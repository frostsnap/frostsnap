//! The one owner of the crypto peripherals that a DS signature interferes with.
//!
//! For the whole of a signature the DS drives the HMAC peripheral (which runs on SHA) and the
//! MPI block that RSA uses. Nothing else may touch SHA, HMAC or RSA from the start of a signature
//! until its finish and clean-up are complete. [`CryptoEngine`] owns all of them, so the only ways
//! in are [`CryptoEngine::idle`], which lends an [`Idle`] only while no signature is in flight,
//! and [`CryptoEngine::for_upgrade`], which finishes any signature before it lends a
//! [`FirmwareChecker`].
//!
//! ```
//! use frostsnap_embedded::crypto_engine::*;
//! fn device_loop<H: CryptoHardware>(
//!     engine: &mut CryptoEngine<H>,
//!     request: SignRequest,
//!     image: &H::FirmwareImage,
//! ) {
//!     if let Some(mut idle) = engine.idle() {
//!         idle.share_encryption().hash("domain", b"input");
//!         idle.queue_attestation(request, b"message");
//!     }
//!     if let Some(_attestation) = engine.poll() {}
//!     let mut firmware = engine.for_upgrade();
//!     firmware.digest(image, None);
//!     let _ = firmware.verify_secure_boot(image);
//! }
//! ```
//!
//! # Misuse that does not compile
//!
//! The hardware is a private field, and the engine has no accessor for it:
//!
//! ```compile_fail,E0616
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     engine.hardware.sha256(b"");
//! }
//! ```
//!
//! ```compile_fail,E0599
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     engine.hardware_mut().sha256(b"");
//! }
//! ```
//!
//! Neither borrow lends the hardware or a raw SHA:
//!
//! ```compile_fail,E0599
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     engine.idle().unwrap().sha256(b"");
//! }
//! ```
//!
//! ```compile_fail,E0599
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     engine.for_upgrade().sha256(b"");
//! }
//! ```
//!
//! ```compile_fail,E0616
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     engine.for_upgrade().engine.hardware.sha256(b"");
//! }
//! ```
//!
//! The firmware checks are not reachable from a handler that holds an `Idle`, neither directly
//! nor through the engine behind it:
//!
//! ```compile_fail,E0599
//! use frostsnap_embedded::crypto_engine::*;
//! fn handler<H: CryptoHardware>(idle: &mut Idle<'_, H>, image: &H::FirmwareImage) {
//!     let _ = idle.verify_secure_boot(image);
//! }
//! ```
//!
//! ```compile_fail,E0616
//! use frostsnap_embedded::crypto_engine::*;
//! fn handler<H: CryptoHardware>(idle: &mut Idle<'_, H>) {
//!     idle.engine.for_upgrade();
//! }
//! ```
//!
//! A handler cannot start or advance a signature either:
//!
//! ```compile_fail,E0599
//! use frostsnap_embedded::crypto_engine::*;
//! fn handler<H: CryptoHardware>(idle: &mut Idle<'_, H>) {
//!     idle.poll();
//! }
//! ```
//!
//! An `Idle`, and anything it lends, cannot be held across `poll`, which may start a signature:
//!
//! ```compile_fail,E0499
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     let mut idle = engine.idle().unwrap();
//!     engine.poll();
//!     idle.share_encryption().hash("domain", b"");
//! }
//! ```
//!
//! ```compile_fail,E0499
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     let mut idle = engine.idle().unwrap();
//!     let mut key = idle.share_encryption();
//!     engine.poll();
//!     key.hash("domain", b"");
//! }
//! ```
//!
//! Nor can an HMAC key be kept while the same `Idle` hashes an attestation message:
//!
//! ```compile_fail,E0499
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>, request: SignRequest) {
//!     let mut idle = engine.idle().unwrap();
//!     let mut key = idle.share_encryption();
//!     idle.queue_attestation(request, b"message");
//!     key.hash("domain", b"");
//! }
//! ```
//!
//! A `FirmwareChecker` cannot be held across `poll`, nor coexist with an `Idle`:
//!
//! ```compile_fail,E0499
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>, image: &H::FirmwareImage) {
//!     let mut firmware = engine.for_upgrade();
//!     engine.poll();
//!     firmware.digest(image, None);
//! }
//! ```
//!
//! ```compile_fail,E0499
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>, image: &H::FirmwareImage) {
//!     let mut firmware = engine.for_upgrade();
//!     let _idle = engine.idle();
//!     firmware.digest(image, None);
//! }
//! ```
//!
//! Neither borrow can be built except by the engine:
//!
//! ```compile_fail,E0451
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     let _idle = Idle { engine };
//! }
//! ```
//!
//! ```compile_fail,E0451
//! use frostsnap_embedded::crypto_engine::*;
//! fn misuse<H: CryptoHardware>(engine: &mut CryptoEngine<H>) {
//!     let _firmware = FirmwareChecker { engine };
//! }
//! ```

use alloc::boxed::Box;
use frostsnap_comms::factory::{pad_message_for_rsa, PaddedMessageBlock};
use frostsnap_comms::genuine_certificate::DsSignature;
use frostsnap_comms::{GenuineChallenge, Sha256Digest};
use frostsnap_core::device::DeviceSecretDerivation;

/// The SHA, HMAC, RSA and DS peripherals, and the factory data that holds the DS parameters.
///
/// Once a [`CryptoEngine`] owns it, only the engine calls these. Before that, boot code may.
pub trait CryptoHardware {
    type HmacKeyId: Copy;
    /// A handle to the factory data. It holds no factory data itself.
    type FactoryData: Copy;
    type FirmwareImage: ?Sized;
    type SecureBootError;

    /// `None` on a device without a DS key.
    fn factory_data(&self) -> Option<Self::FactoryData>;
    fn sha256(&mut self, message: &[u8]) -> [u8; 32];
    fn hmac(&mut self, key: Self::HmacKeyId, domain: &str, input: &[u8]) -> [u8; 32];
    /// Reads the DS parameters from the factory data, starts the exponentiation of `block` and
    /// drops the parameters. The DS is not started if they cannot be read.
    fn ds_start(&mut self, block: &PaddedMessageBlock) -> Result<(), FactoryDataUnreadable>;
    fn ds_busy(&self) -> bool;
    fn ds_read_signature(&mut self) -> DsSignature;
    /// Invalidates the key the HMAC derived for the DS.
    fn hmac_invalidate_ds_key(&mut self);
    fn ds_set_finish(&mut self);
    /// SHA-256 of a firmware image, up to `up_to` bytes.
    fn firmware_digest(&mut self, image: &Self::FirmwareImage, up_to: Option<u32>) -> Sha256Digest;
    /// The secure-boot check of a firmware image against the eFuse key digest.
    fn verify_secure_boot(
        &mut self,
        image: &Self::FirmwareImage,
    ) -> Result<(), Self::SecureBootError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactoryDataUnreadable;

/// What a DS signature is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignRequest {
    FactoryAttestation(GenuineChallenge),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    pub request: SignRequest,
    pub signature: DsSignature,
}

/// What [`Idle::queue_attestation`] did with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Queued {
    Fresh,
    /// The returned request had not started, and will get no reply.
    Superseded(SignRequest),
    Duplicate,
    NoDsKey,
}

#[derive(Debug, Clone, Copy)]
pub struct HmacKeyIds<K> {
    pub share_encryption: K,
    pub fixed_entropy: K,
}

pub struct CryptoEngine<H: CryptoHardware> {
    hardware: H,
    keys: HmacKeyIds<H::HmacKeyId>,
    queued: Option<Box<(SignRequest, PaddedMessageBlock)>>,
    state: State,
}

enum State {
    Idle,
    Computing(SignRequest),
    Finishing(Box<Attestation>),
}

/// A borrow of an idle engine, for the normal loop's handlers.
pub struct Idle<'e, H: CryptoHardware> {
    engine: &'e mut CryptoEngine<H>,
}

/// A borrow of an idle engine, for the exclusive upgrade phase.
pub struct FirmwareChecker<'e, H: CryptoHardware> {
    engine: &'e mut CryptoEngine<H>,
}

/// An eFuse HMAC key. It borrows the hardware for as long as it lives.
pub struct HmacKey<'h, H: CryptoHardware> {
    hardware: &'h mut H,
    key: H::HmacKeyId,
}

impl<H: CryptoHardware> CryptoEngine<H> {
    pub fn new(hardware: H, keys: HmacKeyIds<H::HmacKeyId>) -> Self {
        Self {
            hardware,
            keys,
            queued: None,
            state: State::Idle,
        }
    }

    /// `None` from the start of a signature until its finish and clean-up are complete.
    pub fn idle(&mut self) -> Option<Idle<'_, H>> {
        match self.state {
            State::Idle => Some(Idle { engine: self }),
            State::Computing(_) | State::Finishing(_) => None,
        }
    }

    /// Never waits. Starts the queued signature, or advances the one in flight. Returns the
    /// attestation once, in the call that completes the finish and clean-up, which leaves the
    /// engine idle. A request whose DS parameters can't be read when it starts is dropped.
    pub fn poll(&mut self) -> Option<Attestation> {
        match core::mem::replace(&mut self.state, State::Idle) {
            State::Idle => {
                if let Some(queued) = self.queued.take() {
                    let (request, block) = *queued;
                    if self.hardware.ds_start(&block).is_ok() {
                        self.state = State::Computing(request);
                    }
                }
                None
            }
            State::Computing(request) if self.hardware.ds_busy() => {
                self.state = State::Computing(request);
                None
            }
            State::Computing(request) => {
                let signature = self.hardware.ds_read_signature();
                // ESP-IDF's esp_ds_finish_sign order: hmac_hal_clean, then ds_hal_finish.
                self.hardware.hmac_invalidate_ds_key();
                self.hardware.ds_set_finish();
                self.state = State::Finishing(Box::new(Attestation { request, signature }));
                None
            }
            State::Finishing(attestation) if self.hardware.ds_busy() => {
                self.state = State::Finishing(attestation);
                None
            }
            State::Finishing(attestation) => Some(*attestation),
        }
    }

    /// For entering upgrade mode only. Drops the queued request, waits for any signature in
    /// flight to finish and be cleaned up, and drops its attestation.
    pub fn for_upgrade(&mut self) -> FirmwareChecker<'_, H> {
        self.queued = None;
        while !matches!(self.state, State::Idle) {
            self.poll();
        }
        FirmwareChecker { engine: self }
    }

    pub fn factory_data(&self) -> Option<H::FactoryData> {
        self.hardware.factory_data()
    }
}

impl<H: CryptoHardware> Idle<'_, H> {
    pub fn share_encryption(&mut self) -> HmacKey<'_, H> {
        HmacKey::new(&mut self.engine.hardware, self.engine.keys.share_encryption)
    }

    pub fn fixed_entropy(&mut self) -> HmacKey<'_, H> {
        HmacKey::new(&mut self.engine.hardware, self.engine.keys.fixed_entropy)
    }

    pub fn factory_data(&self) -> Option<H::FactoryData> {
        self.engine.factory_data()
    }

    /// Makes `request` the next attestation, signing the SHA-256 of `message`; the next
    /// [`CryptoEngine::poll`] starts it. It replaces a queued request that has not started, and a
    /// request identical to the queued one is dropped without hashing.
    pub fn queue_attestation(&mut self, request: SignRequest, message: &[u8]) -> Queued {
        let engine = &mut *self.engine;
        if engine.hardware.factory_data().is_none() {
            return Queued::NoDsKey;
        }
        let superseded = match engine.queued.as_deref() {
            Some((queued, _)) if *queued == request => return Queued::Duplicate,
            Some((queued, _)) => Some(*queued),
            None => None,
        };
        let block = pad_message_for_rsa(&engine.hardware.sha256(message));
        engine.queued = Some(Box::new((request, block)));
        match superseded {
            Some(old) => Queued::Superseded(old),
            None => Queued::Fresh,
        }
    }
}

impl<H: CryptoHardware> FirmwareChecker<'_, H> {
    pub fn digest(&mut self, image: &H::FirmwareImage, up_to: Option<u32>) -> Sha256Digest {
        self.engine.hardware.firmware_digest(image, up_to)
    }

    pub fn verify_secure_boot(
        &mut self,
        image: &H::FirmwareImage,
    ) -> Result<(), H::SecureBootError> {
        self.engine.hardware.verify_secure_boot(image)
    }
}

impl<'h, H: CryptoHardware> HmacKey<'h, H> {
    /// For boot code that runs before a [`CryptoEngine`] owns `hardware`.
    pub fn new(hardware: &'h mut H, key: H::HmacKeyId) -> Self {
        Self { hardware, key }
    }

    pub fn hash(&mut self, domain: &str, input: &[u8]) -> [u8; 32] {
        self.hardware.hmac(self.key, domain, input)
    }
}

impl<H: CryptoHardware> DeviceSecretDerivation for HmacKey<'_, H> {
    fn get_share_encryption_key(
        &mut self,
        access_structure_ref: frostsnap_core::AccessStructureRef,
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
        frostsnap_core::SymmetricKey(self.hash("share-encryption", &src))
    }

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: frostsnap_core::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32] {
        let mut input = [0u8; 52];
        input[..16].copy_from_slice(nonce_stream_id.to_bytes().as_slice());
        input[16..20].copy_from_slice(&index.to_be_bytes());
        input[20..52].copy_from_slice(seed_material);
        self.hash("nonce-seed", &input)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::RefCell;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        Sha,
        Hmac(u8),
        DsStart,
        DsRead,
        HmacInvalidateDs,
        DsSetFinish,
        FirmwareDigest,
        SecureBoot,
    }

    #[derive(Default)]
    struct Mock {
        log: Rc<RefCell<Vec<Event>>>,
        has_ds_key: bool,
        params_readable: bool,
        busy_polls: u32,
        busy_left: core::cell::Cell<u32>,
    }

    impl Mock {
        fn record(&self, event: Event) {
            self.log.borrow_mut().push(event);
        }
    }

    impl CryptoHardware for Mock {
        type HmacKeyId = u8;
        type FactoryData = ();
        type FirmwareImage = [u8];
        type SecureBootError = ();

        fn factory_data(&self) -> Option<()> {
            self.has_ds_key.then_some(())
        }
        fn sha256(&mut self, message: &[u8]) -> [u8; 32] {
            self.record(Event::Sha);
            [message.len() as u8; 32]
        }
        fn hmac(&mut self, key: u8, _domain: &str, _input: &[u8]) -> [u8; 32] {
            self.record(Event::Hmac(key));
            [key; 32]
        }
        fn ds_start(&mut self, _block: &PaddedMessageBlock) -> Result<(), FactoryDataUnreadable> {
            assert!(self.has_ds_key);
            if !self.params_readable {
                return Err(FactoryDataUnreadable);
            }
            self.record(Event::DsStart);
            self.busy_left.set(self.busy_polls);
            Ok(())
        }
        fn ds_busy(&self) -> bool {
            let left = self.busy_left.get();
            self.busy_left.set(left.saturating_sub(1));
            left > 0
        }
        fn ds_read_signature(&mut self) -> DsSignature {
            self.record(Event::DsRead);
            DsSignature([7; 384])
        }
        fn hmac_invalidate_ds_key(&mut self) {
            self.record(Event::HmacInvalidateDs);
        }
        fn ds_set_finish(&mut self) {
            self.record(Event::DsSetFinish);
            self.busy_left.set(self.busy_polls);
        }
        fn firmware_digest(&mut self, image: &[u8], _up_to: Option<u32>) -> Sha256Digest {
            self.record(Event::FirmwareDigest);
            Sha256Digest([image.len() as u8; 32])
        }
        fn verify_secure_boot(&mut self, _image: &[u8]) -> Result<(), ()> {
            self.record(Event::SecureBoot);
            Ok(())
        }
    }

    const KEYS: HmacKeyIds<u8> = HmacKeyIds {
        share_encryption: 2,
        fixed_entropy: 3,
    };

    fn engine(has_ds_key: bool) -> (CryptoEngine<Mock>, Rc<RefCell<Vec<Event>>>) {
        let mock = Mock {
            has_ds_key,
            params_readable: true,
            busy_polls: 3,
            ..Mock::default()
        };
        let log = mock.log.clone();
        (CryptoEngine::new(mock, KEYS), log)
    }

    fn request(byte: u8) -> SignRequest {
        SignRequest::FactoryAttestation(GenuineChallenge([byte; 32]))
    }

    fn queue(engine: &mut CryptoEngine<Mock>, request: SignRequest) -> Queued {
        engine
            .idle()
            .expect("idle")
            .queue_attestation(request, b"message")
    }

    fn poll_until_attestation(engine: &mut CryptoEngine<Mock>) -> (Attestation, usize) {
        for polls in 1..100 {
            if let Some(attestation) = engine.poll() {
                return (attestation, polls);
            }
            assert!(engine.idle().is_none(), "nothing is lent mid-signature");
        }
        panic!("signature never completed");
    }

    #[test]
    fn the_signature_is_read_and_cleaned_up_before_the_engine_lends_again() {
        let (mut engine, log) = engine(true);
        assert_eq!(queue(&mut engine, request(1)), Queued::Fresh);
        let (attestation, _) = poll_until_attestation(&mut engine);
        assert_eq!(attestation.request, request(1));
        assert_eq!(attestation.signature, DsSignature([7; 384]));

        engine
            .idle()
            .expect("idle after clean-up")
            .share_encryption()
            .hash("d", b"");
        assert_eq!(
            *log.borrow(),
            [
                Event::Sha,
                Event::DsStart,
                Event::DsRead,
                Event::HmacInvalidateDs,
                Event::DsSetFinish,
                Event::Hmac(2),
            ]
        );
    }

    #[test]
    fn the_attestation_is_returned_only_after_the_finish_clears_busy() {
        let (mut engine, log) = engine(true);
        queue(&mut engine, request(1));
        let mut set_finish_seen_at = None;
        for polls in 1.. {
            if engine.poll().is_some() {
                let set_finish_seen_at = set_finish_seen_at.expect("finish issued first");
                assert!(polls > set_finish_seen_at + 1, "waited for busy to clear");
                break;
            }
            if set_finish_seen_at.is_none() && log.borrow().contains(&Event::DsSetFinish) {
                set_finish_seen_at = Some(polls);
            }
        }
        assert!(engine.idle().is_some());
    }

    #[test]
    fn polling_an_idle_engine_with_nothing_queued_does_nothing() {
        let (mut engine, log) = engine(true);
        assert!(engine.poll().is_none());
        assert!(engine.idle().is_some());
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn a_newer_request_supersedes_a_queued_one_and_an_identical_one_is_not_hashed() {
        let (mut engine, log) = engine(true);
        assert_eq!(queue(&mut engine, request(1)), Queued::Fresh);
        assert_eq!(queue(&mut engine, request(1)), Queued::Duplicate);
        assert_eq!(
            queue(&mut engine, request(2)),
            Queued::Superseded(request(1))
        );
        assert_eq!(log.borrow().iter().filter(|e| **e == Event::Sha).count(), 2);

        let (attestation, _) = poll_until_attestation(&mut engine);
        assert_eq!(attestation.request, request(2));
        assert!(
            engine.poll().is_none(),
            "the superseded request never starts"
        );
        assert_eq!(
            log.borrow()
                .iter()
                .filter(|e| **e == Event::DsStart)
                .count(),
            1
        );
    }

    #[test]
    fn a_request_queued_after_a_signature_is_answered_next() {
        let (mut engine, _) = engine(true);
        queue(&mut engine, request(1));
        assert_eq!(poll_until_attestation(&mut engine).0.request, request(1));
        assert_eq!(queue(&mut engine, request(1)), Queued::Fresh);
        assert_eq!(poll_until_attestation(&mut engine).0.request, request(1));
    }

    #[test]
    fn without_a_ds_key_nothing_is_queued() {
        let (mut engine, log) = engine(false);
        assert_eq!(queue(&mut engine, request(1)), Queued::NoDsKey);
        assert!(engine.poll().is_none());
        assert!(engine.idle().is_some());
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn unreadable_ds_parameters_at_start_drop_the_request() {
        let (mut engine, log) = engine(true);
        engine.hardware.params_readable = false;
        queue(&mut engine, request(1));
        assert!(engine.poll().is_none());
        assert!(engine.idle().is_some());
        engine.hardware.params_readable = true;
        assert!(engine.poll().is_none(), "the request is gone");
        assert!(!log.borrow().contains(&Event::DsStart));
    }

    #[test]
    fn for_upgrade_finishes_and_cleans_up_a_signature_in_flight_before_lending() {
        let (mut engine, log) = engine(true);
        queue(&mut engine, request(1));
        engine.poll();
        assert!(engine.idle().is_none());

        let mut firmware = engine.for_upgrade();
        firmware.digest(b"image", None);
        assert_eq!(firmware.verify_secure_boot(b"image"), Ok(()));
        assert_eq!(
            *log.borrow(),
            [
                Event::Sha,
                Event::DsStart,
                Event::DsRead,
                Event::HmacInvalidateDs,
                Event::DsSetFinish,
                Event::FirmwareDigest,
                Event::SecureBoot,
            ]
        );
        assert!(engine.poll().is_none(), "the attestation was dropped");
        assert!(engine.idle().is_some());
    }

    #[test]
    fn for_upgrade_drops_a_request_that_has_not_started() {
        let (mut engine, log) = engine(true);
        queue(&mut engine, request(1));
        engine.for_upgrade();
        assert!(engine.poll().is_none());
        assert!(!log.borrow().contains(&Event::DsStart));
    }

    #[test]
    fn the_keys_reach_the_hmac_with_their_ids() {
        let (mut engine, log) = engine(false);
        let mut idle = engine.idle().unwrap();
        idle.share_encryption().hash("d", b"");
        idle.fixed_entropy().hash("d", b"");
        assert_eq!(*log.borrow(), [Event::Hmac(2), Event::Hmac(3)]);
    }
}
