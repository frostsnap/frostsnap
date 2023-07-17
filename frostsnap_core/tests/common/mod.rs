use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage,
    DeviceToCoordindatorMessage, DeviceToStorageMessage, DeviceToUserMessage, SignTask,
};
use frostsnap_core::DeviceId;
use std::collections::BTreeSet;

#[derive(Debug)]
pub enum Send {
    DeviceToUser {
        message: DeviceToUserMessage,
        device_id: DeviceId,
    },
    CoordinatorToUser(CoordinatorToUserMessage),
    DeviceToCoordinator(DeviceToCoordindatorMessage),
    CoordinatorToDevice(CoordinatorToDeviceMessage),
    UserToCoordinator(UserToCoordinator),
    ToStorage, /* ignoring these for now */
}

impl From<CoordinatorSend> for Send {
    fn from(value: CoordinatorSend) -> Self {
        match value {
            CoordinatorSend::ToDevice(v) => v.into(),
            CoordinatorSend::ToUser(v) => v.into(),
            CoordinatorSend::ToStorage(_) => Send::ToStorage,
        }
    }
}

impl From<CoordinatorToUserMessage> for Send {
    fn from(value: CoordinatorToUserMessage) -> Self {
        Send::CoordinatorToUser(value)
    }
}

impl From<DeviceToCoordindatorMessage> for Send {
    fn from(value: DeviceToCoordindatorMessage) -> Self {
        Send::DeviceToCoordinator(value)
    }
}

impl From<CoordinatorToDeviceMessage> for Send {
    fn from(value: CoordinatorToDeviceMessage) -> Self {
        Send::CoordinatorToDevice(value)
    }
}

impl From<DeviceToStorageMessage> for Send {
    fn from(_: DeviceToStorageMessage) -> Self {
        Send::ToStorage
    }
}

#[derive(Debug)]
pub enum UserToCoordinator {
    DoKeyGen {
        threshold: usize,
    },
    StartSign {
        message: SignTask,
        devices: BTreeSet<DeviceId>,
    },
}
