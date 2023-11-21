use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToStorageMessage,
    CoordinatorToUserMessage, DeviceSend, DeviceToCoordinatorMessage, DeviceToStorageMessage,
    DeviceToUserMessage,
};
use frostsnap_core::{DeviceId, FrostCoordinator, FrostSigner};
use std::collections::BTreeMap;

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
    CoordinatorToDevice(CoordinatorToDeviceMessage),
    CoordinatorToStorage(CoordinatorToStorageMessage),
    DeviceToStorage {
        from: DeviceId,
        message: DeviceToStorageMessage,
    },
}

impl From<CoordinatorSend> for Send {
    fn from(value: CoordinatorSend) -> Self {
        match value {
            CoordinatorSend::ToDevice(v) => v.into(),
            CoordinatorSend::ToUser(v) => v.into(),
            CoordinatorSend::ToStorage(v) => Send::CoordinatorToStorage(v),
        }
    }
}

impl From<CoordinatorToUserMessage> for Send {
    fn from(value: CoordinatorToUserMessage) -> Self {
        Send::CoordinatorToUser(value)
    }
}

impl From<CoordinatorToDeviceMessage> for Send {
    fn from(value: CoordinatorToDeviceMessage) -> Self {
        Send::CoordinatorToDevice(value)
    }
}

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
    }
    fn storage_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToStorageMessage,
    ) {
    }
    fn storage_react_to_coordinator(
        &mut self,
        run: &mut Run,
        message: CoordinatorToStorageMessage,
    ) {
    }
}

pub struct Run {
    pub coordinator: FrostCoordinator,
    pub devices: BTreeMap<DeviceId, FrostSigner>,
    pub message_stack: Vec<Send>,
    pub transcript: Vec<Send>,
}

impl Run {
    pub fn new(coordinator: FrostCoordinator, devices: BTreeMap<DeviceId, FrostSigner>) -> Self {
        Self {
            coordinator,
            devices,
            message_stack: Default::default(),
            transcript: Default::default(),
        }
    }

    pub fn run_until_finished<E: Env>(&mut self, env: &mut E) {
        self.run_until(env, |_| false)
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

    pub fn run_until<E: Env>(&mut self, env: &mut E, mut until: impl FnMut(&mut Run) -> bool) {
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
                            .recv_device_message(from, message)
                            .unwrap()
                            .into_iter()
                            .map(Send::from),
                    );
                }
                Send::CoordinatorToDevice(message) => {
                    for destination in message.default_destinations() {
                        self.message_stack.extend(
                            self.devices
                                .get_mut(&destination)
                                .unwrap()
                                .recv_coordinator_message(message.clone())
                                .unwrap()
                                .into_iter()
                                .map(|v| Send::device_send(destination, v)),
                        );
                    }
                }
                Send::DeviceToStorage { from, message } => {
                    env.storage_react_to_device(self, from, message);
                }
                Send::CoordinatorToStorage(message) => {
                    env.storage_react_to_coordinator(self, message);
                }
            }
        }
    }
}
