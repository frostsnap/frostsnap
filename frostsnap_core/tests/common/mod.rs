use bitcoin::hashes::{sha256, Hash, HashEngine, Hmac, HmacEngine};
use frostsnap_core::device::{DeviceSecretDerivation, DeviceToUserMessage, KeyPurpose};
use frostsnap_core::message::{
    keygen, CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage,
};
use frostsnap_core::{
    coordinator::{
        CoordinatorSend, CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, FrostCoordinator,
    },
    device::FrostSigner,
    DeviceId, SymmetricKey,
};
use frostsnap_core::{AccessStructureRef, MessageResult};
use rand::RngCore;
use schnorr_fun::frost::ShareIndex;
use schnorr_fun::fun::KeyPair;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const TEST_ENCRYPTION_KEY: SymmetricKey = SymmetricKey([42u8; 32]);

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)] // we're in a test
pub enum Send {
    DeviceToUser {
        message: DeviceToUserMessage,
        from: DeviceId,
    },
    CoordinatorToUser(CoordinatorToUserMessage),
    DeviceToCoordinator {
        from: DeviceId,
        message: DeviceToCoordinatorMessage,
    },
    CoordinatorToDevice {
        destinations: BTreeSet<DeviceId>,
        message: CoordinatorToDeviceMessage,
    },
}

impl From<CoordinatorSend> for Send {
    fn from(value: CoordinatorSend) -> Self {
        match value {
            CoordinatorSend::ToDevice {
                message,
                destinations,
            } => Send::CoordinatorToDevice {
                destinations,
                message,
            },
            CoordinatorSend::ToUser(v) => v.into(),
        }
    }
}

impl From<CoordinatorToUserMessage> for Send {
    fn from(value: CoordinatorToUserMessage) -> Self {
        Send::CoordinatorToUser(value)
    }
}

impl Send {
    pub fn device_send(from: DeviceId, device_send: DeviceSend) -> Self {
        match device_send {
            DeviceSend::ToCoordinator(message) => Send::DeviceToCoordinator {
                from,
                message: *message,
            },
            DeviceSend::ToUser(message) => Send::DeviceToUser {
                message: *message,
                from,
            },
        }
    }
}

pub struct TestDeviceKeyGen;

impl DeviceSecretDerivation for TestDeviceKeyGen {
    fn get_share_encryption_key(
        &mut self,
        _access_structure_ref: AccessStructureRef,
        _party_index: ShareIndex,
        _coord_key: frostsnap_core::CoordShareDecryptionContrib,
    ) -> SymmetricKey {
        TEST_ENCRYPTION_KEY
    }

    fn derive_nonce_seed(
        &mut self,
        nonce_stream_id: frostsnap_core::nonce_stream::NonceStreamId,
        index: u32,
        seed_material: &[u8; 32],
    ) -> [u8; 32] {
        let mut engine = HmacEngine::<sha256::Hash>::new(&TEST_ENCRYPTION_KEY.0);
        engine.input(nonce_stream_id.to_bytes().as_slice());
        engine.input(&index.to_le_bytes());
        engine.input(seed_material);
        let hmac_result = Hmac::<sha256::Hash>::from_engine(engine);

        *hmac_result.as_byte_array()
    }
}

#[allow(unused)]
pub trait Env {
    fn user_react_to_coordinator(
        &mut self,
        run: &mut Run,
        message: CoordinatorToUserMessage,
        rng: &mut impl RngCore,
    ) {
        match message {
            CoordinatorToUserMessage::KeyGen {
                keygen_id,
                inner:
                    CoordinatorToUserKeyGenMessage::KeyGenAck {
                        all_acks_received: true,
                        ..
                    },
            } => {
                let send_finalize_keygen = run
                    .coordinator
                    .finalize_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                    .unwrap();
                run.extend(send_finalize_keygen);
            }
            _ => { /* nothing needs doing */ }
        }
    }
    fn user_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
        rng: &mut impl RngCore,
    ) {
        match message {
            DeviceToUserMessage::FinalizeKeyGen { .. } => {}
            DeviceToUserMessage::CheckKeyGen { phase, .. } => {
                let ack = run
                    .device(from)
                    .keygen_ack(*phase, &mut TestDeviceKeyGen, rng)
                    .unwrap();
                run.extend_from_device(from, ack);
            }
            DeviceToUserMessage::SignatureRequest { phase } => {
                let sign_ack = run
                    .device(from)
                    .sign_ack(*phase, &mut TestDeviceKeyGen)
                    .unwrap();
                run.extend_from_device(from, sign_ack);
            }
            DeviceToUserMessage::Restoration(restoration) => {
                use frostsnap_core::device::restoration::ToUserRestoration::*;
                match restoration {
                    DisplayBackupRequest { phase } => {
                        let backup_ack = run
                            .device(from)
                            .display_backup_ack(*phase, &mut TestDeviceKeyGen)
                            .unwrap();
                        run.extend_from_device(from, backup_ack);
                    }
                    ConsolidateBackup(phase) => {
                        let ack = run.device(from).finish_consolidation(
                            &mut TestDeviceKeyGen,
                            phase,
                            rng,
                        );
                        run.extend_from_device(from, ack);
                    }
                    _ => { /* do nothing */ }
                };
            }
            DeviceToUserMessage::VerifyAddress { .. } => {
                // we dont actually confirm on the device
            }
            _ => { /* do nothing */ }
        }
    }
}

#[derive(Clone)]
pub struct Run {
    pub coordinator: FrostCoordinator,
    pub coordinator_keygen_keypair: KeyPair,
    pub devices: BTreeMap<DeviceId, FrostSigner>,
    pub message_queue: VecDeque<Send>,
    pub transcript: Vec<Send>,
    pub start_coordinator: FrostCoordinator,
    pub start_devices: BTreeMap<DeviceId, FrostSigner>,
}

impl core::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run")
            .field("coordinator", &"..")
            .field("devices", &self.device_set())
            .field("message_queue", &self.message_queue)
            .field("transcript", &self.transcript)
            .field("start_coordinator", &"..")
            .field("start_devices", &"..")
            .finish()
    }
}

impl Run {
    pub fn generate_with_nonce_slots(
        n_devices: usize,
        rng: &mut impl rand_core::RngCore,
        nonce_slots: usize,
    ) -> Self {
        let coordinator_keygen_keypair = FrostCoordinator::short_lived_keygen_keypair(rng);
        let mut coordinator = FrostCoordinator::new();
        coordinator.keygen_fingerprint = schnorr_fun::frost::Fingerprint {
            bits_per_coeff: 2,
            max_bits_total: 6,
            tag: "test",
        };
        Self::new(
            coordinator,
            coordinator_keygen_keypair,
            (0..n_devices)
                .map(|_| {
                    let mut signer = FrostSigner::new_random(rng, nonce_slots);
                    signer.keygen_fingerprint = schnorr_fun::frost::Fingerprint {
                        bits_per_coeff: 2,
                        max_bits_total: 6,
                        tag: "test",
                    };
                    (signer.device_id(), signer)
                })
                .collect(),
        )
    }

    pub fn generate(n_devices: usize, rng: &mut impl rand_core::RngCore) -> Self {
        Self::generate_with_nonce_slots(n_devices, rng, 8)
    }

    #[allow(unused)]
    pub fn start_after_keygen(
        n_devices: usize,
        threshold: u16,
        env: &mut impl Env,
        rng: &mut impl rand_core::RngCore,
        purpose: KeyPurpose,
    ) -> Self {
        let mut run = Self::generate(n_devices, rng);
        let keygen_init = run
            .coordinator
            .begin_keygen(
                keygen::Begin::new(
                    run.devices.keys().cloned().collect::<Vec<_>>(),
                    threshold,
                    "my new key".to_string(),
                    purpose,
                    run.coordinator_keygen_keypair.public_key(),
                    rng,
                ),
                run.coordinator_keygen_keypair,
                rng,
            )
            .unwrap();
        let keygen_id = keygen_init.0.keygen_id;
        run.extend(keygen_init);

        run.run_until_finished(env, rng).unwrap();

        run
    }

    #[allow(unused)]
    pub fn start_after_keygen_and_nonces(
        n_devices: usize,
        threshold: u16,
        env: &mut impl Env,
        rng: &mut impl rand_core::RngCore,
        n_nonce_streams: usize,
        purpose: KeyPurpose,
    ) -> Self {
        let mut run = Self::start_after_keygen(n_devices, threshold, env, rng, purpose);

        run.extend(run.coordinator.maybe_request_nonce_replenishment(
            &run.device_set(),
            n_nonce_streams,
            rng,
        ));
        run.run_until_finished(env, rng).unwrap();

        run
    }

    pub fn new(
        coordinator: FrostCoordinator,
        coordinator_keygen_keypair: KeyPair,
        devices: BTreeMap<DeviceId, FrostSigner>,
    ) -> Self {
        Self {
            start_coordinator: coordinator.clone(),
            coordinator_keygen_keypair,
            start_devices: devices.clone(),
            coordinator,
            devices,
            message_queue: Default::default(),
            transcript: Default::default(),
        }
    }

    #[allow(unused)]
    pub fn replace_coordiantor(&mut self, coordinator: FrostCoordinator) {
        self.coordinator = coordinator.clone();
        self.start_coordinator = coordinator;
    }

    pub fn device_set(&self) -> BTreeSet<DeviceId> {
        self.devices.keys().cloned().collect()
    }

    #[allow(unused)]
    pub fn device_vec(&self) -> Vec<DeviceId> {
        self.devices.keys().cloned().collect()
    }

    pub fn run_until_finished<E: Env>(
        &mut self,
        env: &mut E,
        rng: &mut impl rand_core::RngCore,
    ) -> MessageResult<()> {
        self.run_until(env, rng, |_| false)
    }

    pub fn extend(&mut self, iter: impl IntoIterator<Item = impl Into<Send>>) {
        self.message_queue
            .extend(iter.into_iter().map(|v| v.into()));
    }

    pub fn extend_from_device(
        &mut self,
        from: DeviceId,
        iter: impl IntoIterator<Item = DeviceSend>,
    ) {
        self.message_queue
            .extend(iter.into_iter().map(|v| Send::device_send(from, v)))
    }

    pub fn device(&mut self, id: DeviceId) -> &mut FrostSigner {
        self.devices.get_mut(&id).unwrap()
    }

    pub fn run_until<E: Env>(
        &mut self,
        env: &mut E,
        rng: &mut impl rand_core::RngCore,
        mut until: impl FnMut(&mut Run) -> bool,
    ) -> MessageResult<()> {
        while !until(self) {
            let to_send = match self.message_queue.pop_front() {
                Some(message) => message,
                None => break,
            };

            self.transcript.push(to_send.clone());

            match to_send {
                Send::DeviceToUser { message, from } => {
                    env.user_react_to_device(self, from, message, rng);
                }
                Send::CoordinatorToUser(message) => {
                    env.user_react_to_coordinator(self, message, rng);
                }
                Send::DeviceToCoordinator { from, message } => {
                    self.message_queue.extend(
                        self.coordinator
                            .recv_device_message(from, message)?
                            .into_iter()
                            .map(Send::from),
                    );
                }
                Send::CoordinatorToDevice {
                    destinations,
                    message,
                } => {
                    for destination in destinations {
                        self.message_queue.extend(
                            self.devices
                                .get_mut(&destination)
                                .unwrap()
                                .recv_coordinator_message(
                                    message.clone(),
                                    rng,
                                    &mut TestDeviceKeyGen,
                                )?
                                .into_iter()
                                .map(|v| Send::device_send(destination, v)),
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub fn check_mutations(&mut self) {
        let mutations = self.coordinator.take_staged_mutations();

        for mutation in mutations {
            self.start_coordinator.apply_mutation(mutation);
        }
        assert_eq!(
            self.start_coordinator,
            {
                let mut tmp = self.coordinator.clone();
                tmp.clear_tmp_data();
                tmp
            },
            "coordinator should be the same after applying mutations"
        );

        for (device_id, device) in &mut self.devices {
            let mut device = device.clone();
            device.clear_tmp_data();
            let mutations = device.staged_mutations().drain(..).collect::<Vec<_>>();
            let start_device = self.start_devices.get_mut(device_id).unwrap();
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

impl Drop for Run {
    fn drop(&mut self) {
        self.check_mutations();
    }
}
