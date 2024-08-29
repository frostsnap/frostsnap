use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage, DeviceSend,
    DeviceToCoordinatorMessage, DeviceToStorageMessage, DeviceToUserMessage,
};
use frostsnap_core::{coordinator, MessageResult};
use frostsnap_core::{
    coordinator::{FrostCoordinator, SigningSessionState},
    DeviceId, FrostSigner,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
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
    DeviceToStorage {
        from: DeviceId,
        message: DeviceToStorageMessage,
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

// impl From<CoordinatorToDeviceMessage> for Send {
//     fn from(value: CoordinatorToDeviceMessage) -> Self {
//         Send::CoordinatorToDevice {
//             destinations: BTreeSet::new(),
//             message: value,
//         }
//     }
// }

impl Send {
    pub fn device_send(from: DeviceId, device_send: DeviceSend) -> Self {
        match device_send {
            DeviceSend::ToCoordinator(message) => Send::DeviceToCoordinator { from, message },
            DeviceSend::ToUser(message) => Send::DeviceToUser { message, from },
            DeviceSend::ToStorage(message) => Send::DeviceToStorage { from, message },
        }
    }
}

#[allow(unused)]
pub trait Env {
    fn user_react_to_coordinator(&mut self, run: &mut Run, message: CoordinatorToUserMessage) {}
    fn user_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
    ) {
        match message {
            DeviceToUserMessage::CheckKeyGen { .. } => {
                let ack = run.device(from).keygen_ack().unwrap();
                run.extend_from_device(from, ack);
            }
            DeviceToUserMessage::SignatureRequest { .. } => {
                let sign_ack = run.device(from).sign_ack().unwrap();
                run.extend_from_device(from, sign_ack);
            }
            DeviceToUserMessage::DisplayBackupRequest { .. } => {
                let backup_ack = run.device(from).display_backup_ack().unwrap();
                run.extend_from_device(from, backup_ack);
            }
            DeviceToUserMessage::Canceled { .. } => {
                panic!("no cancelling done");
            }
            _ => { /* do nothing */ }
        }
    }
    fn storage_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToStorageMessage,
    ) {
    }
    fn storage_react_to_coordinator_mutation(
        &mut self,
        run: &mut Run,
        message: coordinator::Mutation,
    ) {
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
    pub message_stack: Vec<Send>,
    pub transcript: Vec<Send>,
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
            coordinator,
            devices,
            message_stack: Default::default(),
            transcript: Default::default(),
        }
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
        self.message_stack
            .extend(iter.into_iter().map(|v| v.into()));
    }

    pub fn extend_from_device(
        &mut self,
        from: DeviceId,
        iter: impl IntoIterator<Item = DeviceSend>,
    ) {
        self.message_stack
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
            let to_send = match self.message_stack.pop() {
                Some(message) => message,
                None => break,
            };

            self.transcript.push(to_send.clone());

            match to_send {
                Send::DeviceToUser { message, from } => {
                    env.user_react_to_device(self, from, message);
                }
                Send::CoordinatorToUser(message) => {
                    env.user_react_to_coordinator(self, message);
                }
                Send::DeviceToCoordinator { from, message } => {
                    self.message_stack.extend(
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
                        self.message_stack.extend(
                            self.devices
                                .get_mut(&destination)
                                .unwrap()
                                .recv_coordinator_message(message.clone(), rng)?
                                .into_iter()
                                .map(|v| Send::device_send(destination, v)),
                        );
                    }
                }
                Send::DeviceToStorage { from, message } => {
                    env.storage_react_to_device(self, from, message);
                }
                Send::CoordinatorSigningSession(signing_session_state) => {
                    env.sign_session_state_react_to_coordinator(self, signing_session_state);
                }
            }

            for mutation in self.coordinator.take_staged_mutations() {
                env.storage_react_to_coordinator_mutation(self, mutation);
            }
        }

        Ok(())
    }
}
