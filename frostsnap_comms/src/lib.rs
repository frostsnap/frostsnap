#![no_std]

#[cfg(feature = "std")]
#[allow(unused)]
#[macro_use]
extern crate std;

#[allow(unused)]
#[macro_use]
extern crate alloc;
use alloc::vec::Vec;
use alloc::{collections::BTreeSet, string::String};
use bincode::{de::read::Reader, enc::write::Writer, Decode, Encode};
use core::marker::PhantomData;
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
    pub target_destinations: BTreeSet<DeviceId>,
    pub message_body: DeviceReceiveBody,
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceReceiveBody {
    Core(CoordinatorToDeviceMessage),
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

impl<D> DeviceSendSerial<D> {
    pub fn gist(&self) -> &'static str {
        match self {
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
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceSendMessage {
    Core(DeviceToCoordindatorMessage),
    Debug { message: String, device: DeviceId },
    Announce(Announce),
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct Announce {
    pub from: DeviceId,
}

pub fn make_progress_on_magic_bytes<D: Direction>(
    remaining: &[u8],
    progress: usize,
) -> (usize, usize, bool) {
    let magic_bytes = D::magic_bytes_recv();
    _make_progress_on_magic_bytes(remaining, &magic_bytes, progress)
}

fn _make_progress_on_magic_bytes(
    remaining: &[u8],
    magic_bytes: &[u8],
    mut progress: usize,
) -> (usize, usize, bool) {
    let mut consumed = 0;

    for byte in remaining.iter() {
        consumed += 1;
        if *byte == magic_bytes[progress] {
            progress += 1;
            if progress == magic_bytes.len() {
                return (consumed, 0, true);
            }
        } else {
            progress = 0;
        }
    }

    (consumed, progress, false)
}

pub fn find_and_remove_magic_bytes<D: Direction>(buff: &mut Vec<u8>) -> bool {
    let magic_bytes = D::magic_bytes_recv();
    _find_and_remove_magic_bytes(buff, &magic_bytes[..])
}

fn _find_and_remove_magic_bytes(buff: &mut Vec<u8>, magic_bytes: &[u8]) -> bool {
    let (consumed, _, found) = _make_progress_on_magic_bytes(&buff[..], magic_bytes, 0);

    if found {
        *buff = buff.split_off(consumed);
    }

    found
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
        assert_eq!(bytes, b"magic world");

        let mut bytes = b"magic".to_vec();
        assert!(_find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b"");
    }
}
