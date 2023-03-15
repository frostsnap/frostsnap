#![no_std]

use bincode::{Decode, Encode};
use frostsnap_core::message::{CoordinatorToDeviceSend, DeviceToCoordindatorMessage};

#[derive(Encode, Decode, Debug, Clone)]
pub struct DeviceReceiveSerial {
    #[bincode(with_serde)]
    pub to_device_send: CoordinatorToDeviceSend,
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct DeviceSendSerial {
    #[bincode(with_serde)]
    pub message: DeviceToCoordindatorMessage,
}

// #[derive(Encode, Debug, Clone)]
// struct CoordinatorSendSerial {
//     #[bincode(with_serde)]
//     message: CoordinatorToDeviceSend,
// }

// #[derive(Decode, Debug, Clone)]
// struct CoordinatorReceiveSerial {
//     #[bincode(with_serde)]
//     message: DeviceToCoordindatorMessage,
// }
