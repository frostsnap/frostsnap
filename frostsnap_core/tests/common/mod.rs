use frostsnap_core::device::DeviceSymmetricKeyGen;
use frostsnap_core::message::{
    CoordinatorToDeviceMessage, CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage,
    DeviceSend, DeviceToCoordinatorMessage, DeviceToUserMessage,
};
use frostsnap_core::MessageResult;
use frostsnap_core::{
    coordinator::{CoordinatorSend, FrostCoordinator, SigningSessionState},
    device::FrostSigner,
    DeviceId, SymmetricKey,
};
use rand::RngCore;
use schnorr_fun::frost::PartyIndex;
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
    CoordinatorSigningSession(SigningSessionState),
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
            CoordinatorSend::SigningSessionStore(session_state) => {
                Send::CoordinatorSigningSession(session_state)
            }
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

impl DeviceSymmetricKeyGen for TestDeviceKeyGen {
    fn get_share_encryption_key(
        &mut self,
        _key_id: frostsnap_core::KeyId,
        _access_structure_id: frostsnap_core::AccessStructureId,
        _party_index: PartyIndex,
        _coord_key: frostsnap_core::CoordShareDecryptionContrib,
    ) -> SymmetricKey {
        TEST_ENCRYPTION_KEY
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
            CoordinatorToUserMessage::KeyGen(CoordinatorToUserKeyGenMessage::KeyGenAck {
                all_acks_received: true,
                ..
            }) => {
                run.coordinator.final_keygen_ack(TEST_ENCRYPTION_KEY, rng);
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
            DeviceToUserMessage::CheckKeyGen { .. } => {
                let ack = run
                    .device(from)
                    .keygen_ack(&mut TestDeviceKeyGen, rng)
                    .unwrap();
                run.extend_from_device(from, ack);
            }
            DeviceToUserMessage::SignatureRequest { .. } => {
                let sign_ack = run.device(from).sign_ack(&mut TestDeviceKeyGen).unwrap();
                run.extend_from_device(from, sign_ack);
            }
            DeviceToUserMessage::DisplayBackupRequest { .. } => {
                let backup_ack = run
                    .device(from)
                    .display_backup_ack(&mut TestDeviceKeyGen)
                    .unwrap();
                run.extend_from_device(from, backup_ack);
            }
            DeviceToUserMessage::Canceled { .. } => {
                panic!("no cancelling done");
            }
            DeviceToUserMessage::VerifyAddress { .. } => {
                // we dont actually confirm on the device
            }
            _ => { /* do nothing */ }
        }
    }
    fn sign_session_state_react_to_coordinator(
        &mut self,
        run: &mut Run,
        message: SigningSessionState,
    ) {
    }
}

pub struct DefaultTestEnv;

impl Env for DefaultTestEnv {}

pub struct Run {
    pub coordinator: FrostCoordinator,
    pub devices: BTreeMap<DeviceId, FrostSigner>,
    pub message_queue: VecDeque<Send>,
    pub transcript: Vec<Send>,
    pub start_coordinator: FrostCoordinator,
    pub start_devices: BTreeMap<DeviceId, FrostSigner>,
}

impl Run {
    pub fn generate(n_devices: usize, rng: &mut impl rand_core::RngCore) -> Self {
        Self::new(
            FrostCoordinator::new(),
            (0..n_devices)
                .map(|_| {
                    let signer = FrostSigner::new_random(rng);
                    (signer.device_id(), signer)
                })
                .collect(),
        )
    }
    pub fn new(coordinator: FrostCoordinator, devices: BTreeMap<DeviceId, FrostSigner>) -> Self {
        Self {
            start_coordinator: coordinator.clone(),
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
                                .recv_coordinator_message(message.clone(), rng)?
                                .into_iter()
                                .map(|v| Send::device_send(destination, v)),
                        );
                    }
                }
                Send::CoordinatorSigningSession(signing_session_state) => {
                    env.sign_session_state_react_to_coordinator(self, signing_session_state);
                }
            }
        }

        Ok(())
    }

    pub fn check_mutations(&mut self) {
        let mutations = self.coordinator.take_staged_mutations();

        for mutation in mutations {
            self.start_coordinator.apply_mutation(&mutation);
        }
        assert_eq!(
            self.start_coordinator,
            {
                let mut tmp = self.coordinator.clone();
                tmp.cancel();
                tmp
            },
            "coordinator should be the same after applying mutations"
        );

        for (device_id, device) in &mut self.devices {
            let mut device = device.clone();
            let _ = device.cancel_action();
            let mutations = device.staged_mutations().drain(..);
            let start_device = self.start_devices.get_mut(device_id).unwrap();
            for mutation in mutations {
                start_device.apply_mutation(&mutation);
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
