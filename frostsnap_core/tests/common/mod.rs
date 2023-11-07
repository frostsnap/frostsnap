use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage, DeviceSend,
    DeviceToCoordinatorMessage, DeviceToStorageMessage, DeviceToUserMessage, SignTask,
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
    DeviceToCoordinator {
        from: DeviceId,
        message: DeviceToCoordinatorMessage,
    },
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

impl Send {
    pub fn device_send(device_id: DeviceId, device_send: DeviceSend) -> Self {
        match device_send {
            DeviceSend::ToCoordinator(message) => Send::DeviceToCoordinator {
                from: device_id,
                message,
            },
            DeviceSend::ToUser(message) => Send::DeviceToUser { message, device_id },
            DeviceSend::ToStorage(m) => m.into(),
        }
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
