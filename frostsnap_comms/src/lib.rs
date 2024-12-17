#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[macro_use]
extern crate alloc;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::{collections::BTreeSet, string::String};
use bincode::{de::read::Reader, enc::write::Writer, Decode, Encode};
use core::marker::PhantomData;
use frostsnap_core::{DeviceId, Gist};

pub const BAUDRATE: u32 = 14_400;
/// Magic bytes are 7 bytes in length so when the bincode prefixes it with `00` it is 8 bytes long.
/// A nice round number here is desirable (but not strictly necessary) because TX and TX buffers
/// will be some multiple of 8 and so it should overflow the ring buffers neatly.
const MAGIC_BYTES_LEN: usize = 7;

const MAGICBYTES_RECV_DOWNSTREAM: [u8; MAGIC_BYTES_LEN] =
    [0xff, 0xe4, 0x31, 0xb8, 0x02, 0x8b, 0x06];
const MAGICBYTES_RECV_UPSTREAM: [u8; MAGIC_BYTES_LEN] = [0xff, 0x5d, 0xa3, 0x85, 0xd4, 0xee, 0x5a];

/// Write magic bytes once every 100ms
pub const MAGIC_BYTES_PERIOD: u64 = 100;

pub const FIRMWARE_UPGRADE_CHUNK_LEN: u32 = 4096;

/// This value comes from partitions.csv
pub const FIRMWARE_IMAGE_SIZE: u32 = 0x140_000;

pub const FIRMWARE_NEXT_CHUNK_READY_SIGNAL: u8 = 0x11;

const MAX_MESSAGE_SIZE: usize = 1 << 13;

pub const BINCODE_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Varint,
    bincode::config::Limit<MAX_MESSAGE_SIZE>,
> = bincode::config::standard().with_limit::<MAX_MESSAGE_SIZE>();

#[derive(Encode, Decode, Debug, Clone)]
#[bincode(bounds = "D: Direction")]
pub enum ReceiveSerial<D: Direction> {
    MagicBytes(MagicBytes<D>),
    Message(D::RecvType),
    Unused9,
    Unused8,
    Unused7,
    Unused6,
    Unused5,
    Unused4,
    Unused3,
    Unused2,
    Unused1,
    Unused0,
}

impl<D: Direction> Gist for ReceiveSerial<D> {
    fn gist(&self) -> String {
        match self {
            ReceiveSerial::MagicBytes(_) => "MagicBytes".into(),
            ReceiveSerial::Message(msg) => msg.gist(),
            _ => "Unused".into(),
        }
    }
}

/// A message sent from a coordinator
#[derive(Encode, Decode, Debug, Clone)]
pub struct CoordinatorSendMessage {
    pub target_destinations: Destination,
    pub message_body: WireCoordinatorSendBody,
}

#[cfg(feature = "coordinator")]
impl TryFrom<frostsnap_core::coordinator::CoordinatorSend> for CoordinatorSendMessage {
    type Error = &'static str;

    fn try_from(value: frostsnap_core::coordinator::CoordinatorSend) -> Result<Self, Self::Error> {
        match value {
            frostsnap_core::coordinator::CoordinatorSend::ToDevice {
                message,
                destinations,
            } => Ok(CoordinatorSendMessage {
                target_destinations: Destination::from(destinations),
                message_body: CoordinatorSendBody::Core(message).into(),
            }),
            _ => Err("was not a ToDevice message"),
        }
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum Destination {
    /// Send to all devices -- this reduces message size for this common task
    All,
    Particular(BTreeSet<DeviceId>),
}

impl Destination {
    pub fn should_forward(&self) -> bool {
        match self {
            Self::All => true,
            Self::Particular(devices) => !devices.is_empty(),
        }
    }

    /// Returns whether the arugment `device_id` was a destination
    pub fn remove_from_recipients(&mut self, device_id: DeviceId) -> bool {
        match self {
            Destination::All => true,
            Destination::Particular(devices) => devices.remove(&device_id),
        }
    }

    pub fn is_destined_to(&mut self, device_id: DeviceId) -> bool {
        match self {
            Destination::All => true,
            Destination::Particular(devices) => devices.contains(&device_id),
        }
    }
}

impl Gist for Destination {
    fn gist(&self) -> String {
        match self {
            Destination::All => "ALL".into(),
            Destination::Particular(device_ids) => device_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

impl<I: IntoIterator<Item = DeviceId>> From<I> for Destination {
    fn from(iter: I) -> Self {
        Destination::Particular(BTreeSet::from_iter(iter))
    }
}

impl Gist for CoordinatorSendMessage {
    fn gist(&self) -> String {
        self.message_body.gist()
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum WireCoordinatorSendBody {
    _Core,
    _Naming,
    // ↑ Coord will never send these to old devices -- it will force firmware upgrade
    AnnounceAck,
    Cancel,
    Upgrade(CoordinatorUpgradeMessage),
    /// Everything should be encapsulated on the wire. The above is for backwards compat.
    EncapsV0(EncapsBody),
}

impl WireCoordinatorSendBody {
    pub fn decode(self) -> Option<CoordinatorSendBody> {
        use WireCoordinatorSendBody::*;
        match self {
            _Core | _Naming => None,
            AnnounceAck => Some(CoordinatorSendBody::AnnounceAck),
            Cancel => Some(CoordinatorSendBody::Cancel),
            Upgrade(upgrade) => Some(CoordinatorSendBody::Upgrade(upgrade)),
            EncapsV0(encaps) => bincode::decode_from_slice(encaps.0.as_ref(), BINCODE_CONFIG)
                .ok()
                .map(|(body, _)| body),
        }
    }
}

#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub struct EncapsBody(Vec<u8>);

#[derive(Encode, Decode, Debug, Clone)]
pub enum CoordinatorSendBody {
    Core(frostsnap_core::message::CoordinatorToDeviceMessage),
    Naming(NameCommand),
    AnnounceAck,
    Cancel,
    Upgrade(CoordinatorUpgradeMessage),
}

impl From<CoordinatorSendBody> for WireCoordinatorSendBody {
    fn from(value: CoordinatorSendBody) -> Self {
        use CoordinatorSendBody::*;
        match value {
            Core(_) | Naming(_) => WireCoordinatorSendBody::EncapsV0(EncapsBody(
                bincode::encode_to_vec(value, BINCODE_CONFIG).expect("encoding is infallible"),
            )),
            AnnounceAck => WireCoordinatorSendBody::AnnounceAck,
            Cancel => WireCoordinatorSendBody::Cancel,
            Upgrade(upgrade) => WireCoordinatorSendBody::Upgrade(upgrade),
        }
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum CoordinatorUpgradeMessage {
    PrepareUpgrade {
        size: u32,
        firmware_digest: FirmwareDigest,
    },
    EnterUpgradeMode,
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum NameCommand {
    Preview(String),
    Finish(String),
}

impl Gist for CoordinatorSendBody {
    fn gist(&self) -> String {
        match self {
            CoordinatorSendBody::Core(core) => core.gist(),
            _ => format!("{:?}", self),
        }
    }
}

impl Gist for WireCoordinatorSendBody {
    fn gist(&self) -> String {
        match self {
            WireCoordinatorSendBody::EncapsV0(_) => "EncapsV0".into(),
            _ => match self.clone().decode() {
                Some(decoded) => decoded.gist(),
                None => "UNINTELLIGBLE".into(),
            },
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

/// Message sent from a device to the coordinator
#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub struct DeviceSendMessage {
    pub from: DeviceId,
    pub body: WireDeviceSendBody,
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
    Announce { firmware_digest: FirmwareDigest },
    SetName { name: String },
    DisconnectDownstream,
    NeedName,
    AckUpgradeMode,
}

#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub enum WireDeviceSendBody {
    _Core,
    Debug { message: String },
    Announce { firmware_digest: FirmwareDigest },
    SetName { name: String },
    DisconnectDownstream,
    NeedName,
    AckUpgradeMode,
    EncapsV0(EncapsBody),
}

impl Gist for WireDeviceSendBody {
    fn gist(&self) -> String {
        match self {
            WireDeviceSendBody::EncapsV0(_) => "EncapsV0(..)".into(),
            _ => self.clone().decode().expect("infallible").gist(),
        }
    }
}

impl From<DeviceSendBody> for WireDeviceSendBody {
    fn from(value: DeviceSendBody) -> Self {
        let encaps = bincode::encode_to_vec(value, BINCODE_CONFIG).expect("encoding works");
        WireDeviceSendBody::EncapsV0(EncapsBody(encaps))
    }
}

impl WireDeviceSendBody {
    pub fn decode(self) -> Option<DeviceSendBody> {
        Some(match self {
            WireDeviceSendBody::_Core => return None,
            WireDeviceSendBody::Debug { message } => DeviceSendBody::Debug { message },
            WireDeviceSendBody::Announce { firmware_digest } => {
                DeviceSendBody::Announce { firmware_digest }
            }
            WireDeviceSendBody::SetName { name } => DeviceSendBody::SetName { name },
            WireDeviceSendBody::DisconnectDownstream => DeviceSendBody::DisconnectDownstream,
            WireDeviceSendBody::NeedName => DeviceSendBody::NeedName,
            WireDeviceSendBody::AckUpgradeMode => DeviceSendBody::AckUpgradeMode,
            WireDeviceSendBody::EncapsV0(encaps) => {
                match bincode::decode_from_slice(encaps.0.as_ref(), BINCODE_CONFIG).ok() {
                    Some((msg, _)) => msg,
                    None => return None,
                }
            }
        })
    }
}

impl Gist for DeviceSendBody {
    fn gist(&self) -> String {
        match self {
            DeviceSendBody::Core(msg) => msg.gist(),
            DeviceSendBody::Debug { message } => format!("debug: {message}"),
            DeviceSendBody::DisconnectDownstream
            | DeviceSendBody::NeedName
            | DeviceSendBody::Announce { .. }
            | DeviceSendBody::AckUpgradeMode { .. }
            | DeviceSendBody::SetName { .. } => format!("{:?}", self),
        }
    }
}

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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct FirmwareDigest(pub [u8; 32]);

frostsnap_core::impl_display_debug_serialize! {
    fn to_bytes(digest: &FirmwareDigest) -> [u8;32] {
        digest.0
    }
}

frostsnap_core::impl_fromstr_deserialize! {
    name => "firmware digest",
    fn from_bytes(bytes: [u8;32]) -> FirmwareDigest {
        FirmwareDigest(bytes)
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
        assert_eq!(bytes, b"magic world");

        let mut bytes = b"magic".to_vec();
        assert!(_find_and_remove_magic_bytes(&mut bytes, b"magic"));
        assert_eq!(bytes, b"");
    }
}
