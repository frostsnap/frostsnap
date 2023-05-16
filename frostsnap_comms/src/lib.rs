#![no_std]

#[cfg(feature = "std")]
#[allow(unused)]
#[macro_use]
extern crate std;

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
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
}

impl DeviceReceiveSerial {
    pub fn gist(&self) -> String {
        match self {
            DeviceReceiveSerial::Core(message) => message.kind(),
            DeviceReceiveSerial::AnnounceAck(_) => "AnnounceAck",
        }
        .into()
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceSendSerial {
    Core(#[bincode(with_serde)] DeviceToCoordindatorMessage),
    Debug {
        message: String,
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

pub fn find_and_remove_magic_bytes(buff: &mut Vec<u8>, magic_bytes: &[u8]) -> bool {
    let position = buff
        .windows(magic_bytes.len())
        .position(|window| window == &magic_bytes[..]);
    if let Some(mut position) = position {
        while buff.len() >= magic_bytes.len()
            && &buff[position..position + magic_bytes.len()] == magic_bytes
        {
            *buff = buff.split_off(position + magic_bytes.len());
            position = 0;
        }
        true
    } else {
        false
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn remove_magic_bytes() {
        let mut bytes = b"hello world".to_vec();
        assert!(!find_and_remove_magic_bytes(&mut bytes, b"magic"));

        let mut bytes = b"hello magic world".to_vec();

        assert!(find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b" world");

        let mut bytes = b"hello magicmagic world".to_vec();
        assert!(find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b" world");

        let mut bytes = b"magic".to_vec();
        assert!(find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b"");
    }
}
