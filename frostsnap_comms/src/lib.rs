#![no_std]

use bincode::{Decode, Encode};
use frostsnap_core::{
    message::{CoordinatorToDeviceMessage, DeviceToCoordindatorMessage},
    DeviceId,
};

#[derive(Encode, Decode, Debug, Clone)]
pub struct DeviceReceiveSerial {
    #[bincode(with_serde)]
    pub to_device_send: CoordinatorToDeviceMessage,
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct DeviceSendSerial {
    #[bincode(with_serde)]
    pub message: DeviceToCoordindatorMessage,
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct Announce {
    #[bincode(with_serde)]
    pub from: DeviceId,
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct AnnounceAck {
    #[bincode(with_serde)]
    pub from: DeviceId,
}
