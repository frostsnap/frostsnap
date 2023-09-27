#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[macro_use]
extern crate alloc;
use alloc::vec::Vec;
use alloc::{collections::BTreeSet, string::String};
use bincode::{de::read::Reader, enc::write::Writer, Decode, Encode};
use core::marker::PhantomData;
use frostsnap_core::{DeviceId, Gist};

pub const BAUDRATE: u32 = 9600;
/// Magic bytes are 7 bytes in length so when the bincode prefixes it with `00` it is 8 bytes long.
/// A nice round number here is desirable (but not strictly necessary) because TX and TX buffers
/// will be some multiple of 8 and so it should overflow the ring buffers neatly.
const MAGIC_BYTES_LEN: usize = 7;

const MAGICBYTES_RECV_DOWNSTREAM: [u8; MAGIC_BYTES_LEN] =
    [0xff, 0xe4, 0x31, 0xb8, 0x02, 0x8b, 0x06];
const MAGICBYTES_RECV_UPSTREAM: [u8; MAGIC_BYTES_LEN] = [0xff, 0x5d, 0xa3, 0x85, 0xd4, 0xee, 0x5a];

/// Write magic bytes once every 100ms
pub const MAGIC_BYTES_PERIOD: u64 = 100;

#[derive(Encode, Decode, Debug, Clone)]
#[bincode(bounds = "D: Direction")]
pub enum ReceiveSerial<D: Direction> {
    MagicBytes(MagicBytes<D>),
    Message(D::RecvType),
}

impl<D: Direction> Gist for ReceiveSerial<D> {
    fn gist(&self) -> String {
        match self {
            ReceiveSerial::MagicBytes(_) => "MagicBytes".into(),
            ReceiveSerial::Message(msg) => msg.gist(),
        }
    }
}

/// A message sent from a coordinator
#[derive(Encode, Decode, Debug, Clone)]
pub struct CoordinatorSendMessage {
    pub target_destinations: BTreeSet<DeviceId>,
    pub message_body: CoordinatorSendBody,
}

impl Gist for CoordinatorSendMessage {
    fn gist(&self) -> String {
        self.message_body.gist()
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum CoordinatorSendBody {
    Core(frostsnap_core::message::CoordinatorToDeviceMessage),
    AnnounceAck { device_label: String },
}

impl Gist for CoordinatorSendBody {
    fn gist(&self) -> String {
        match self {
            CoordinatorSendBody::Core(core) => core.gist(),
            CoordinatorSendBody::AnnounceAck { device_label } => {
                format!("AnnoucneAck({})", device_label)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MagicBytes<O>(PhantomData<O>);

impl<O> Default for MagicBytes<O> {
    fn default() -> Self {
        Self(Default::default())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Upstream;
#[derive(Clone, Copy, Debug, Default)]
pub struct Downstream;

pub trait Direction {
    type RecvType: bincode::Decode + bincode::Encode + for<'a> bincode::BorrowDecode<'a> + Gist;
    type Opposite: Direction;
    const MAGIC_BYTES_RECV: [u8; MAGIC_BYTES_LEN];
}

impl Direction for Upstream {
    type RecvType = CoordinatorSendMessage;
    type Opposite = Downstream;
    const MAGIC_BYTES_RECV: [u8; MAGIC_BYTES_LEN] = MAGICBYTES_RECV_UPSTREAM;
}

impl Direction for Downstream {
    type RecvType = DeviceSendMessage;
    type Opposite = Upstream;
    const MAGIC_BYTES_RECV: [u8; MAGIC_BYTES_LEN] = MAGICBYTES_RECV_DOWNSTREAM;
}

impl<O: Direction> bincode::Encode for MagicBytes<O> {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        encoder.writer().write(&O::MAGIC_BYTES_RECV)
    }
}

impl<O: Direction> bincode::Decode for MagicBytes<O> {
    fn decode<D: bincode::de::Decoder>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let mut bytes = [0u8; MAGIC_BYTES_LEN];
        decoder.reader().read(&mut bytes)?;
        if bytes == O::MAGIC_BYTES_RECV {
            Ok(MagicBytes(PhantomData))
        } else {
            Err(bincode::error::DecodeError::OtherString(format!(
                "was expecting magic bytes {:02x?} but got {:02x?}",
                O::MAGIC_BYTES_RECV,
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

/// Message sent from a device
#[derive(Encode, Decode, Debug, Clone)]
pub struct DeviceSendMessage {
    pub from: DeviceId,
    pub body: DeviceSendBody,
}

impl Gist for DeviceSendMessage {
    fn gist(&self) -> String {
        format!("{} <= {}", self.body.gist(), self.from)
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceSendBody {
    Core(frostsnap_core::message::DeviceToCoordinatorMessage),
    Debug { message: String },
    Announce(Announce),
    DisconnectDownstream,
}

impl Gist for DeviceSendBody {
    fn gist(&self) -> String {
        match self {
            DeviceSendBody::Core(msg) => msg.gist(),
            DeviceSendBody::Debug { message } => format!("debug: {message}"),
            DeviceSendBody::Announce(_) => "Announce".into(),
            DeviceSendBody::DisconnectDownstream => "DisconnectedDownstream".into(),
        }
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct Announce {}

pub fn make_progress_on_magic_bytes<D: Direction>(
    remaining: impl Iterator<Item = u8>,
    progress: usize,
) -> (usize, bool) {
    let magic_bytes = D::MAGIC_BYTES_RECV;
    _make_progress_on_magic_bytes(remaining, &magic_bytes, progress)
}

fn _make_progress_on_magic_bytes(
    remaining: impl Iterator<Item = u8>,
    magic_bytes: &[u8],
    mut progress: usize,
) -> (usize, bool) {
    for byte in remaining {
        if byte == magic_bytes[progress] {
            progress += 1;
            if progress == magic_bytes.len() {
                return (0, true);
            }
        } else {
            progress = 0;
        }
    }

    (progress, false)
}

pub fn find_and_remove_magic_bytes<D: Direction>(buff: &mut Vec<u8>) -> bool {
    let magic_bytes = D::MAGIC_BYTES_RECV;
    _find_and_remove_magic_bytes(buff, &magic_bytes[..])
}

fn _find_and_remove_magic_bytes(buff: &mut Vec<u8>, magic_bytes: &[u8]) -> bool {
    let mut consumed = 0;
    let (_, found) = _make_progress_on_magic_bytes(
        buff.iter().cloned().inspect(|_| consumed += 1),
        magic_bytes,
        0,
    );

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
