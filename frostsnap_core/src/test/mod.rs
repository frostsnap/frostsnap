//! Test harness for driving coordinators and devices through the FROST protocol.
//!
//! The top of this module — `Run`, `Env`, `Send`, `Participant` — models a set
//! of coordinators, each with its own local devices, plus a shared message queue
//! that carries messages between them. `run_until_finished` delivers messages
//! in random order, cascading through `recv_device_message` / `recv_coordinator_message`
//! / `apply_keygen_message` until the queue drains.
//!
//! The `BroadcastInterceptor` hook lets downstream crates divert
//! `CoordinatorSend::Broadcast` messages out of the in-memory queue (e.g. to publish
//! them over nostr), while still using `Run` for the coordinator↔device routing.

pub mod env;

pub use env::TestEnv;

use crate::coordinator::remote_keygen::RemoteKeygenMessage;
use crate::coordinator::BroadcastPayload;
use crate::device::{DeviceSecretDerivation, DeviceToUserMessage, KeyPurpose};
use crate::message::{CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage};
use crate::{
    coordinator::{
        BeginKeygen, CoordinatorSend, CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage,
        FrostCoordinator,
    },
    device::FrostSigner,
    DeviceId, KeygenId, SymmetricKey,
};
use crate::{AccessStructureRef, MessageResult};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::ToString,
    vec,
    vec::Vec,
};
use bitcoin::hashes::{sha256, Hash, HashEngine, Hmac, HmacEngine};
use rand_core::RngCore;
use schnorr_fun::frost::ShareIndex;
use schnorr_fun::fun::{KeyPair, Scalar};

pub const TEST_ENCRYPTION_KEY: SymmetricKey = SymmetricKey([42u8; 32]);

pub const TEST_FINGERPRINT: schnorr_fun::frost::Fingerprint = schnorr_fun::frost::Fingerprint {
    bits_per_coeff: 2,
    max_bits_total: 6,
    tag: "test",
};

// ============================================================================
// Outbound broadcasts — for tests that route broadcasts manually
// ============================================================================

/// A `CoordinatorSend::Broadcast` emitted by a coordinator, to be routed by
/// the test (e.g. published over nostr). Only produced when `Run` is in
/// manual broadcast routing mode.
#[derive(Debug, Clone)]
pub struct OutboundBroadcast {
    pub coordinator_index: usize,
    pub channel: KeygenId,
    pub from: DeviceId,
    pub payload: BroadcastPayload,
}

// ============================================================================
// Send enum — shared by Run and RunSingleCoordinator
// ============================================================================

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Send {
    DeviceToUser {
        from: DeviceId,
        message: DeviceToUserMessage,
    },
    CoordinatorToUser {
        coordinator_index: usize,
        message: CoordinatorToUserMessage,
    },
    DeviceToCoordinator {
        from: DeviceId,
        message: DeviceToCoordinatorMessage,
    },
    CoordinatorToDevice {
        coordinator_index: usize,
        destinations: BTreeSet<DeviceId>,
        message: CoordinatorToDeviceMessage,
    },
    Broadcast {
        to: usize,
        channel: KeygenId,
        from: DeviceId,
        payload: BroadcastPayload,
    },
}

impl Send {
    pub fn device_send(from: DeviceId, device_send: DeviceSend) -> Self {
        match device_send {
            DeviceSend::ToCoordinator(message) => Send::DeviceToCoordinator {
                from,
                message: *message,
            },
            DeviceSend::ToUser(message) => Send::DeviceToUser {
                from,
                message: *message,
            },
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

pub struct TestDeviceKeyGen;

impl DeviceSecretDerivation for TestDeviceKeyGen {
    fn get_share_encryption_key(
        &mut self,
        _access_structure_ref: AccessStructureRef,
        _party_index: ShareIndex,
        _coord_key: crate::CoordShareDecryptionContrib,
    ) -> SymmetricKey {
        TEST_ENCRYPTION_KEY
    }

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: crate::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32] {
        let mut engine = HmacEngine::<sha256::Hash>::new(&TEST_ENCRYPTION_KEY.0);
        engine.input(nonce_stream_id.to_bytes().as_slice());
        engine.input(&index.to_le_bytes());
        engine.input(seed_material);
        *Hmac::<sha256::Hash>::from_engine(engine).as_byte_array()
    }
}

// ============================================================================
// Run — multi-coordinator
// ============================================================================

#[derive(Clone)]
pub struct Participant {
    pub coordinator: FrostCoordinator,
    pub devices: BTreeMap<DeviceId, FrostSigner>,
    pub keypair: KeyPair,
    pub start_coordinator: FrostCoordinator,
    pub start_devices: BTreeMap<DeviceId, FrostSigner>,
}

#[allow(unused)]
pub trait Env {
    fn user_react_to_coordinator(
        &mut self,
        run: &mut Run,
        coordinator_index: usize,
        message: CoordinatorToUserMessage,
        rng: &mut impl RngCore,
    ) {
        if let CoordinatorToUserMessage::KeyGen {
            keygen_id,
            inner:
                CoordinatorToUserKeyGenMessage::KeyGenAck {
                    all_acks_received: true,
                    ..
                },
        } = message
        {
            let sends = run.participants[coordinator_index]
                .coordinator
                .finalize_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                .unwrap();
            run.extend_from_coordinator(coordinator_index, sends);
        }
    }

    fn user_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
        rng: &mut impl RngCore,
    ) {
        let ci = run.owner_of(from);
        match message {
            DeviceToUserMessage::FinalizeKeyGen { .. } => {}
            DeviceToUserMessage::CheckKeyGen { phase, .. } => {
                let ack = run.participants[ci]
                    .devices
                    .get_mut(&from)
                    .unwrap()
                    .keygen_ack(*phase, &mut TestDeviceKeyGen, rng)
                    .unwrap();
                run.extend_from_device(from, ack);
            }
            DeviceToUserMessage::SignatureRequest { phase } => {
                let sign_ack = run.participants[ci]
                    .devices
                    .get_mut(&from)
                    .unwrap()
                    .sign_ack(*phase, &mut TestDeviceKeyGen)
                    .unwrap();
                run.extend_from_device(from, sign_ack);
            }
            DeviceToUserMessage::Restoration(restoration) => {
                use crate::device::restoration::ToUserRestoration::*;
                if let ConsolidateBackup(phase) = *restoration {
                    let ack = run.participants[ci]
                        .devices
                        .get_mut(&from)
                        .unwrap()
                        .finish_consolidation(&mut TestDeviceKeyGen, phase, rng);
                    run.extend_from_device(from, ack);
                }
            }
            DeviceToUserMessage::VerifyAddress { .. } => {}
            DeviceToUserMessage::NonceJobs(mut batch) => {
                batch.run_until_finished(&mut TestDeviceKeyGen);
                let segments = batch.into_segments();
                let response =
                    DeviceSend::ToCoordinator(Box::new(DeviceToCoordinatorMessage::Signing(
                        crate::message::signing::DeviceSigning::NonceResponse { segments },
                    )));
                run.extend_from_device(from, vec![response]);
            }
            _ => {}
        }
    }
}

pub struct Run {
    pub participants: Vec<Participant>,
    pub device_owner: BTreeMap<DeviceId, usize>,
    pub message_queue: VecDeque<Send>,
    pub transcript: Vec<Send>,
    /// When true, `CoordinatorSend::Broadcast`s from `extend_from_coordinator`
    /// are diverted into `outbound_broadcasts` instead of being fanned out to
    /// the in-memory `message_queue`. Tests that carry broadcasts over a real
    /// transport (e.g. nostr) enable this and drain `outbound_broadcasts`
    /// themselves between delivery ticks.
    pub manual_broadcast_routing: bool,
    /// Queue of broadcasts that need to be sent by the test. Only populated
    /// when `manual_broadcast_routing` is true.
    pub outbound_broadcasts: VecDeque<OutboundBroadcast>,
}

impl core::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run")
            .field("n_participants", &self.participants.len())
            .field("message_queue_len", &self.message_queue.len())
            .finish()
    }
}

impl Clone for Run {
    fn clone(&self) -> Self {
        Self {
            participants: self.participants.clone(),
            device_owner: self.device_owner.clone(),
            message_queue: self.message_queue.clone(),
            transcript: self.transcript.clone(),
            manual_broadcast_routing: self.manual_broadcast_routing,
            outbound_broadcasts: self.outbound_broadcasts.clone(),
        }
    }
}

#[allow(unused)]
impl Run {
    pub fn new(participants: Vec<Participant>) -> Self {
        let mut device_owner = BTreeMap::new();
        for (i, p) in participants.iter().enumerate() {
            for device_id in p.devices.keys() {
                device_owner.insert(*device_id, i);
            }
        }
        Self {
            participants,
            device_owner,
            message_queue: Default::default(),
            transcript: Default::default(),
            manual_broadcast_routing: false,
            outbound_broadcasts: Default::default(),
        }
    }

    pub fn new_single(
        coordinator: FrostCoordinator,
        devices: BTreeMap<DeviceId, FrostSigner>,
    ) -> Self {
        // Single-coordinator tests don't use `keypair` — it only matters for
        // remote keygen where participants identify themselves to each other.
        // Use a deterministic placeholder.
        let keypair = KeyPair::new(
            Scalar::from_bytes([1u8; 32])
                .expect("1s is a valid scalar")
                .non_zero()
                .expect("non-zero"),
        );
        let p = Participant {
            start_coordinator: coordinator.clone(),
            start_devices: devices.clone(),
            coordinator,
            devices,
            keypair,
        };
        Self::new(vec![p])
    }

    pub fn generate_remote(device_counts: &[usize], rng: &mut impl RngCore) -> Self {
        let participants = device_counts
            .iter()
            .map(|&count| {
                let keypair = KeyPair::new(Scalar::random(rng));
                let mut coordinator = FrostCoordinator::new();
                coordinator.keygen_fingerprint = TEST_FINGERPRINT;
                let devices: BTreeMap<DeviceId, FrostSigner> = (0..count)
                    .map(|_| {
                        let mut signer = FrostSigner::new_random(rng, 8);
                        signer.keygen_fingerprint = TEST_FINGERPRINT;
                        (signer.device_id(), signer)
                    })
                    .collect();
                Participant {
                    start_coordinator: coordinator.clone(),
                    start_devices: devices.clone(),
                    coordinator,
                    devices,
                    keypair,
                }
            })
            .collect();
        Self::new(participants)
    }

    /// Enable manual broadcast routing. Subsequent `CoordinatorSend::Broadcast`s
    /// from `extend_from_coordinator` are pushed to `outbound_broadcasts`
    /// instead of being fanned out to `message_queue`. The test is responsible
    /// for draining `outbound_broadcasts` and carrying the payloads over its
    /// own transport.
    pub fn with_manual_broadcast_routing(mut self) -> Self {
        self.manual_broadcast_routing = true;
        self
    }

    /// Drain all pending outbound broadcasts. Only meaningful when
    /// `manual_broadcast_routing` is enabled.
    pub fn drain_outbound_broadcasts(&mut self) -> VecDeque<OutboundBroadcast> {
        core::mem::take(&mut self.outbound_broadcasts)
    }

    /// Inject a keygen message that arrived out-of-band (e.g. from nostr) into
    /// this `Run`'s coordinator at `to`, exactly as `Send::Broadcast` would.
    /// Any `CoordinatorSend`s produced are enqueued via `extend_from_coordinator`.
    pub fn inject_keygen_message(
        &mut self,
        to: usize,
        channel: KeygenId,
        msg: RemoteKeygenMessage,
    ) -> MessageResult<()> {
        let outgoing = self.participants[to]
            .coordinator
            .apply_keygen_message(channel, msg)?;
        self.extend_from_coordinator(to, outgoing);
        Ok(())
    }

    pub fn all_device_ids(&self) -> Vec<DeviceId> {
        self.participants
            .iter()
            .flat_map(|p| p.devices.keys().copied())
            .collect()
    }

    pub fn coordinator_ids(&self) -> Vec<DeviceId> {
        self.participants
            .iter()
            .map(|p| DeviceId(p.keypair.public_key().to_bytes()))
            .collect()
    }

    pub fn start_remote_keygen(&mut self, begin_keygen: BeginKeygen, rng: &mut impl RngCore) {
        let coordinator_ids = self.coordinator_ids();

        let all_sends: Vec<(usize, Vec<CoordinatorSend>)> = self
            .participants
            .iter_mut()
            .enumerate()
            .map(|(i, p)| {
                let local_devices = p
                    .devices
                    .keys()
                    .filter(|d| begin_keygen.device_to_share_index.contains_key(d))
                    .copied()
                    .collect();
                let sends: Vec<_> = p
                    .coordinator
                    .begin_remote_keygen(
                        begin_keygen.clone(),
                        &coordinator_ids,
                        &local_devices,
                        p.keypair,
                        rng,
                    )
                    .unwrap()
                    .into_iter()
                    .collect();
                (i, sends)
            })
            .collect();

        for (i, sends) in all_sends {
            self.extend_from_coordinator(i, sends);
        }
    }

    pub fn start_after_remote_keygen(
        device_counts: &[usize],
        threshold: u16,
        key_name: &str,
        purpose: KeyPurpose,
        env: &mut impl Env,
        rng: &mut impl RngCore,
    ) -> Self {
        let mut run = Self::generate_remote(device_counts, rng);
        let begin = BeginKeygen::new(
            run.all_device_ids(),
            threshold,
            key_name.to_string(),
            purpose,
            rng,
        );
        run.start_remote_keygen(begin, rng);
        run.run_until_finished(env, rng).unwrap();
        run
    }

    pub fn replenish_all_nonces(
        &mut self,
        n_nonce_streams: usize,
        env: &mut impl Env,
        rng: &mut impl RngCore,
    ) {
        for i in 0..self.participants.len() {
            let device_set: BTreeSet<DeviceId> =
                self.participants[i].devices.keys().copied().collect();
            let replenish = self.participants[i]
                .coordinator
                .maybe_request_nonce_replenishment(&device_set, n_nonce_streams, rng);
            self.extend_from_coordinator(i, replenish);
        }
        self.run_until_finished(env, rng).unwrap();
    }

    pub fn owner_of(&self, device_id: DeviceId) -> usize {
        *self.device_owner.get(&device_id).expect("device not found")
    }

    pub fn extend_from_coordinator(
        &mut self,
        ci: usize,
        iter: impl IntoIterator<Item = impl Into<CoordinatorSend>>,
    ) {
        let local_devices: BTreeSet<DeviceId> =
            self.participants[ci].devices.keys().copied().collect();
        for send in iter {
            match send.into() {
                CoordinatorSend::ToDevice {
                    message,
                    destinations,
                } => {
                    assert!(
                        destinations.is_subset(&local_devices),
                        "coordinator {ci} emitted ToDevice for devices it doesn't own: {:?}",
                        destinations.difference(&local_devices).collect::<Vec<_>>()
                    );
                    self.message_queue.push_back(Send::CoordinatorToDevice {
                        coordinator_index: ci,
                        destinations,
                        message,
                    });
                }
                CoordinatorSend::ToUser(message) => {
                    self.message_queue.push_back(Send::CoordinatorToUser {
                        coordinator_index: ci,
                        message,
                    });
                }
                CoordinatorSend::Broadcast {
                    channel,
                    from,
                    payload,
                } => {
                    if self.manual_broadcast_routing {
                        self.outbound_broadcasts.push_back(OutboundBroadcast {
                            coordinator_index: ci,
                            channel,
                            from,
                            payload,
                        });
                    } else {
                        for j in 0..self.participants.len() {
                            self.message_queue.push_back(Send::Broadcast {
                                to: j,
                                channel,
                                from,
                                payload: payload.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn extend_from_device(
        &mut self,
        from: DeviceId,
        iter: impl IntoIterator<Item = DeviceSend>,
    ) {
        self.message_queue
            .extend(iter.into_iter().map(|v| Send::device_send(from, v)))
    }

    pub fn deliver_one_message<E: Env>(
        &mut self,
        index: usize,
        env: &mut E,
        rng: &mut impl RngCore,
    ) -> MessageResult<()> {
        let to_send = self
            .message_queue
            .remove(index)
            .expect("invalid message index");
        self.transcript.push(to_send.clone());

        match to_send {
            Send::DeviceToUser { from, message } => {
                env.user_react_to_device(self, from, message, rng);
            }
            Send::CoordinatorToUser {
                coordinator_index: ci,
                message,
            } => {
                env.user_react_to_coordinator(self, ci, message, rng);
            }
            Send::DeviceToCoordinator { from, message } => {
                let ci = self.owner_of(from);
                let outgoing = self.participants[ci]
                    .coordinator
                    .recv_device_message(from, message)?;
                self.extend_from_coordinator(ci, outgoing);
            }
            Send::CoordinatorToDevice {
                coordinator_index: ci,
                destinations,
                message,
            } => {
                let p = &mut self.participants[ci];
                for dest in destinations {
                    let sends = p
                        .devices
                        .get_mut(&dest)
                        .unwrap()
                        .recv_coordinator_message(message.clone(), rng)?;
                    self.message_queue
                        .extend(sends.into_iter().map(|v| Send::device_send(dest, v)));
                }
            }
            Send::Broadcast {
                to,
                channel,
                from,
                payload,
            } => match payload {
                BroadcastPayload::RemoteKeygen(keygen_payload) => {
                    let msg = RemoteKeygenMessage {
                        from,
                        payload: keygen_payload,
                    };
                    let outgoing = self.participants[to]
                        .coordinator
                        .apply_keygen_message(channel, msg)?;
                    self.extend_from_coordinator(to, outgoing);
                }
            },
        }
        Ok(())
    }

    pub fn deliver_random_message<E: Env>(
        &mut self,
        env: &mut E,
        rng: &mut impl RngCore,
    ) -> MessageResult<bool> {
        if self.message_queue.is_empty() {
            return Ok(false);
        }
        let index = rng.next_u64() as usize % self.message_queue.len();
        self.deliver_one_message(index, env, rng)?;
        Ok(true)
    }

    pub fn run_until_finished<E: Env>(
        &mut self,
        env: &mut E,
        rng: &mut impl RngCore,
    ) -> MessageResult<()> {
        self.run_until(env, rng, |_| false)
    }

    pub fn run_until<E: Env>(
        &mut self,
        env: &mut E,
        rng: &mut impl RngCore,
        mut until: impl FnMut(&mut Run) -> bool,
    ) -> MessageResult<()> {
        while !until(self) {
            if !self.deliver_random_message(env, rng)? {
                break;
            }
        }
        Ok(())
    }

    pub fn check_mutations(&mut self) {
        for p in &mut self.participants {
            let mutations = p.coordinator.take_staged_mutations();
            for mutation in mutations {
                p.start_coordinator.apply_mutation(mutation);
            }
            assert_eq!(
                p.start_coordinator,
                {
                    let mut tmp = p.coordinator.clone();
                    tmp.clear_tmp_data();
                    tmp
                },
                "coordinator should be the same after applying mutations"
            );

            for (device_id, device) in &mut p.devices {
                let mut device = device.clone();
                device.clear_tmp_data();
                let mutations = device.staged_mutations().drain(..).collect::<Vec<_>>();
                let start_device = p.start_devices.get_mut(device_id).unwrap();
                *start_device.nonce_slots() = device.nonce_slots().clone();
                for mutation in mutations {
                    start_device.apply_mutation(mutation);
                }
                assert_eq!(
                    *start_device, device,
                    "device should be the same after applying mutations"
                );
            }
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.check_mutations();
    }
}

// ============================================================================
// RunSingleCoordinator — wraps Run, exposes backward-compatible API
// ============================================================================

#[allow(unused)]
pub struct RunSingleCoordinator(pub Run);

impl core::fmt::Debug for RunSingleCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Clone for RunSingleCoordinator {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl core::ops::Deref for RunSingleCoordinator {
    type Target = Participant;
    fn deref(&self) -> &Self::Target {
        &self.0.participants[0]
    }
}

impl core::ops::DerefMut for RunSingleCoordinator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.participants[0]
    }
}

#[allow(unused)]
impl RunSingleCoordinator {
    pub fn message_queue(&self) -> &VecDeque<Send> {
        &self.0.message_queue
    }
    pub fn message_queue_mut(&mut self) -> &mut VecDeque<Send> {
        &mut self.0.message_queue
    }
    pub fn transcript(&self) -> &Vec<Send> {
        &self.0.transcript
    }

    // Construction
    pub fn new(coordinator: FrostCoordinator, devices: BTreeMap<DeviceId, FrostSigner>) -> Self {
        Self(Run::new_single(coordinator, devices))
    }

    pub fn generate_with_nonce_slots_and_batch_size(
        n_devices: usize,
        rng: &mut impl RngCore,
        nonce_slots: usize,
        nonce_batch_size: u32,
    ) -> Self {
        let mut coordinator = FrostCoordinator::new();
        coordinator.keygen_fingerprint = TEST_FINGERPRINT;
        Self::new(
            coordinator,
            (0..n_devices)
                .map(|_| {
                    let mut signer = FrostSigner::new_random_with_nonce_batch_size(
                        rng,
                        nonce_slots,
                        nonce_batch_size,
                    );
                    signer.keygen_fingerprint = TEST_FINGERPRINT;
                    (signer.device_id(), signer)
                })
                .collect(),
        )
    }

    pub fn generate(n_devices: usize, rng: &mut impl RngCore) -> Self {
        Self::generate_with_nonce_slots_and_batch_size(n_devices, rng, 8, 10)
    }

    pub fn start_after_keygen(
        n_devices: usize,
        threshold: u16,
        env: &mut impl Env,
        rng: &mut impl RngCore,
        purpose: KeyPurpose,
    ) -> Self {
        let mut run = Self::generate(n_devices, rng);
        use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        let mut coordinator_rng = ChaCha20Rng::from_seed(seed);

        let device_ids: Vec<_> = run.devices.keys().cloned().collect();
        let keygen_init = run
            .coordinator
            .begin_keygen(
                BeginKeygen::new(
                    device_ids,
                    threshold,
                    "my new key".to_string(),
                    purpose,
                    rng,
                ),
                &mut coordinator_rng,
            )
            .unwrap();
        run.extend(keygen_init);
        run.run_until_finished(env, rng).unwrap();
        run
    }

    pub fn start_after_keygen_and_nonces(
        n_devices: usize,
        threshold: u16,
        env: &mut impl Env,
        rng: &mut impl RngCore,
        n_nonce_streams: usize,
        purpose: KeyPurpose,
    ) -> Self {
        let mut run = Self::start_after_keygen(n_devices, threshold, env, rng, purpose);
        let device_set = run.device_set();
        let replenish =
            run.coordinator
                .maybe_request_nonce_replenishment(&device_set, n_nonce_streams, rng);
        run.extend(replenish);
        run.run_until_finished(env, rng).unwrap();
        run
    }

    /// Enable manual broadcast routing on the underlying `Run`.
    pub fn with_manual_broadcast_routing(mut self) -> Self {
        self.0.manual_broadcast_routing = true;
        self
    }

    pub fn inject_keygen_message(
        &mut self,
        channel: KeygenId,
        msg: RemoteKeygenMessage,
    ) -> MessageResult<()> {
        self.0.inject_keygen_message(0, channel, msg)
    }

    // Delegation
    pub fn clear_coordinator(&mut self) {
        let mut c = FrostCoordinator::new();
        c.keygen_fingerprint = TEST_FINGERPRINT;
        self.0.participants[0].coordinator = c.clone();
        self.0.participants[0].start_coordinator = c;
    }

    pub fn new_device(&mut self, rng: &mut impl RngCore) -> DeviceId {
        let mut signer = FrostSigner::new_random_with_nonce_batch_size(rng, 8, 10);
        signer.keygen_fingerprint = TEST_FINGERPRINT;
        let device_id = signer.device_id();
        self.0.participants[0]
            .devices
            .insert(device_id, signer.clone());
        self.0.participants[0]
            .start_devices
            .insert(device_id, signer);
        self.0.device_owner.insert(device_id, 0);
        device_id
    }

    pub fn device_set(&self) -> BTreeSet<DeviceId> {
        self.devices.keys().cloned().collect()
    }

    pub fn device_vec(&self) -> Vec<DeviceId> {
        self.devices.keys().cloned().collect()
    }

    pub fn device(&mut self, id: DeviceId) -> &mut FrostSigner {
        self.devices.get_mut(&id).unwrap()
    }

    pub fn extend(&mut self, iter: impl IntoIterator<Item = impl Into<CoordinatorSend>>) {
        self.0.extend_from_coordinator(0, iter);
    }

    pub fn extend_from_device(
        &mut self,
        from: DeviceId,
        iter: impl IntoIterator<Item = DeviceSend>,
    ) {
        self.0.extend_from_device(from, iter)
    }

    pub fn run_until_finished<E: Env>(
        &mut self,
        env: &mut E,
        rng: &mut impl RngCore,
    ) -> MessageResult<()> {
        self.0.run_until_finished(env, rng)
    }

    pub fn run_until<E: Env>(
        &mut self,
        env: &mut E,
        rng: &mut impl RngCore,
        mut until: impl FnMut(&mut RunSingleCoordinator) -> bool,
    ) -> MessageResult<()> {
        loop {
            if until(self) {
                break;
            }
            if self.0.message_queue.is_empty() {
                break;
            }
            let mut done = false;
            self.0.run_until(env, rng, |_| {
                if done {
                    return true;
                }
                done = true;
                false
            })?;
        }
        Ok(())
    }

    pub fn check_mutations(&mut self) {
        self.0.check_mutations()
    }
}

impl Drop for RunSingleCoordinator {
    fn drop(&mut self) {
        // Run's Drop already checks mutations — idempotent, so nothing to do here.
    }
}
