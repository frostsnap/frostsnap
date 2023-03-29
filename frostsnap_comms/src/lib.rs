#![no_std]

extern crate alloc;

use alloc::string::String;
use bincode::{Decode, Encode};
use frostsnap_core::{
    message::{CoordinatorToDeviceMessage, DeviceToCoordindatorMessage},
    DeviceId,
};

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceReceiveSerial {
    Core(#[bincode(with_serde)] CoordinatorToDeviceMessage),
    AnnounceAck(#[bincode(with_serde)] DeviceId),
    AnnounceCoordinator(String),
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceSendSerial {
    Core(#[bincode(with_serde)] DeviceToCoordindatorMessage),
    Debug {
        error: String,
        #[bincode(with_serde)]
        device: DeviceId,
    },
    Announce(Announce),
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct Announce {
    #[bincode(with_serde)]
    pub from: DeviceId,
}
