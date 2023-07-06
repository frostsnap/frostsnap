#![no_std]

#[cfg(feature = "std")]
#[allow(unused)]
#[macro_use]
extern crate std;

#[allow(unused)]
#[macro_use]
extern crate alloc;
use core::marker::PhantomData;

use alloc::vec::Vec;
use alloc::{collections::BTreeSet, string::String};
use bincode::{de::read::Reader, enc::write::Writer, Decode, Encode};
use frostsnap_core::{
    message::{CoordinatorToDeviceMessage, DeviceToCoordinatorBody, DeviceToCoordindatorMessage},
    DeviceId,
};

pub const BAUDRATE: u32 = 9600;
/// Magic bytes are 7 bytes in length so when the bincode prefixes it with `00` it is 8 bytes long.
/// A nice round number here is desirable (but not strictly necessary) because TX and TX buffers
/// will be some multiple of 8 and so it should overflow the ring buffers neatly.
const MAGIC_BYTES_LEN: usize = 7;

const MAGICBYTES_SEND_UPSTREAM: [u8; MAGIC_BYTES_LEN] = [0xff, 0xe4, 0x31, 0xb8, 0x02, 0x8b, 0x06];
const MAGICBYTES_SEND_DOWNSTREAM: [u8; MAGIC_BYTES_LEN] =
    [0xff, 0x5d, 0xa3, 0x85, 0xd4, 0xee, 0x5a];

#[derive(Encode, Decode, Debug, Clone)]
#[bincode(bounds = "D: Direction")]
pub enum DeviceReceiveSerial<D> {
    MagicBytes(MagicBytes<D>),
    Message(DeviceReceiveMessage),
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct DeviceReceiveMessage {
    #[bincode(with_serde)]
    pub target_destinations: BTreeSet<DeviceId>,
    pub message_body: DeviceReceiveBody,
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceReceiveBody {
    Core(#[bincode(with_serde)] CoordinatorToDeviceMessage),
    AnnounceAck { device_label: String },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MagicBytes<O>(PhantomData<O>);

#[derive(Clone, Copy, Debug, Default)]
pub struct Upstream;
#[derive(Clone, Copy, Debug, Default)]
pub struct Downstream;

pub trait Direction {
    fn magic_bytes_send() -> [u8; MAGIC_BYTES_LEN];
    fn magic_bytes_recv() -> [u8; MAGIC_BYTES_LEN];
}

impl Direction for Upstream {
    fn magic_bytes_send() -> [u8; MAGIC_BYTES_LEN] {
        MAGICBYTES_SEND_UPSTREAM
    }

    fn magic_bytes_recv() -> [u8; MAGIC_BYTES_LEN] {
        MAGICBYTES_SEND_DOWNSTREAM
    }
}

impl Direction for Downstream {
    fn magic_bytes_send() -> [u8; MAGIC_BYTES_LEN] {
        MAGICBYTES_SEND_DOWNSTREAM
    }

    fn magic_bytes_recv() -> [u8; MAGIC_BYTES_LEN] {
        MAGICBYTES_SEND_UPSTREAM
    }
}

impl<O: Direction> bincode::Encode for MagicBytes<O> {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        encoder.writer().write(&O::magic_bytes_send())
    }
}

impl<O: Direction> bincode::Decode for MagicBytes<O> {
    fn decode<D: bincode::de::Decoder>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let mut bytes = [0u8; MAGIC_BYTES_LEN];
        decoder.reader().read(&mut bytes)?;
        if bytes == O::magic_bytes_recv() {
            Ok(MagicBytes(PhantomData))
        } else {
            Err(bincode::error::DecodeError::OtherString(format!(
                "was expecting magic bytes {:02x?} but got {:02x?}",
                O::magic_bytes_recv(),
                bytes
            )))
        }
    }
}

impl<'de, O: Direction> bincode::BorrowDecode<'de> for MagicBytes<O> {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(
        decoder: &mut D,
    ) -> core::result::Result<Self, bincode::error::DecodeError> {
        bincode::Decode::decode(decoder)
    }
}

impl<D> DeviceReceiveSerial<D> {
    pub fn gist(&self) -> String {
        match self {
            DeviceReceiveSerial::MagicBytes(_) => "MagicBytes".into(),
            DeviceReceiveSerial::Message(message) => match &message.message_body {
                DeviceReceiveBody::Core(message) => message.kind().into(),
                DeviceReceiveBody::AnnounceAck { .. } => "AnnounceAck".into(),
            },
        }
    }
}

#[derive(Encode, Decode, Debug, Clone)]
#[bincode(bounds = "D: Direction")]
pub enum DeviceSendSerial<D> {
    MagicBytes(MagicBytes<D>),
    Message(DeviceSendMessage),
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceSendMessage {
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

pub fn find_and_remove_magic_bytes<D: Direction>(buff: &mut Vec<u8>) -> bool {
    let magic_bytes = D::magic_bytes_recv();
    _find_and_remove_magic_bytes(buff, &magic_bytes[..])
}

fn _find_and_remove_magic_bytes(buff: &mut Vec<u8>, magic_bytes: &[u8]) -> bool {
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

pub fn gist_send<D>(send: &DeviceSendSerial<D>) -> &'static str {
    match send {
        DeviceSendSerial::MagicBytes(_) => "MagicBytes",
        DeviceSendSerial::Message(message) => match message {
            DeviceSendMessage::Core(message) => match message.body {
                DeviceToCoordinatorBody::KeyGenResponse(_) => "KeyGenResponse",
                DeviceToCoordinatorBody::SignatureShare { .. } => "SignatureShare",
            },
            DeviceSendMessage::Debug { .. } => "Debug",
            DeviceSendMessage::Announce(_) => "Announce",
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn remove_magic_bytes() {
        let mut bytes = b"hello world".to_vec();
        assert!(!_find_and_remove_magic_bytes(&mut bytes, b"magic"));

        let mut bytes = b"hello magic world".to_vec();

        assert!(_find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b" world");

        let mut bytes = b"hello magicmagic world".to_vec();
        assert!(_find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b" world");

        let mut bytes = b"magic".to_vec();
        assert!(_find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b"");
    }
}
