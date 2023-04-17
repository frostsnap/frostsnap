#![no_std]

extern crate alloc;

use alloc::string::String;
use bincode::{Decode, Encode};
use frostsnap_core::{
    message::{CoordinatorToDeviceMessage, DeviceToCoordindatorMessage},
    DeviceId,
};

pub const BAUDRATE: u32 = 9600;

pub const MAGICBYTES_JTAG: [u8; 4] = [0xb, 0xe, 0xe, 0xf];
pub const MAGICBYTES_UART: [u8; 4] = [0xa, 0xa, 0xa, 0xa];

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