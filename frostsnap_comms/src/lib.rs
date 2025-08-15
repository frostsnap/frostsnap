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

pub const BAUDRATE: u32 = 115_200;
/// Magic bytes are 7 bytes in length so when the bincode prefixes it with `00` it is 8 bytes long.
/// A nice round number here is desirable (but not strictly necessary) because TX and TX buffers
/// will be some multiple of 8 and so it should overflow the ring buffers neatly.
///
/// The last byte of magic bytes is used to signal features (by incrementing for new features).
pub const MAGIC_BYTES_LEN: usize = 7;

const MAGICBYTES_RECV_DOWNSTREAM: [u8; MAGIC_BYTES_LEN] =
    [0xff, 0xe4, 0x31, 0xb8, 0x02, 0x8b, 0x06];
const MAGICBYTES_RECV_UPSTREAM: [u8; MAGIC_BYTES_LEN] = [0xff, 0x5d, 0xa3, 0x85, 0xd4, 0xee, 0x5a];

/// Write magic bytes once every 100ms
pub const MAGIC_BYTES_PERIOD: u64 = 100;

pub const FIRMWARE_UPGRADE_CHUNK_LEN: u32 = 4096;

/// This value comes from partitions.csv
pub const FIRMWARE_IMAGE_SIZE: u32 = 0x140_000;

pub const FIRMWARE_NEXT_CHUNK_READY_SIGNAL: u8 = 0x11;

/// Max memory we should use when deocding a message
const MAX_MESSAGE_ALLOC_SIZE: usize = 1 << 15;

pub const BINCODE_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Varint,
    bincode::config::Limit<MAX_MESSAGE_ALLOC_SIZE>,
> = bincode::config::standard().with_limit::<MAX_MESSAGE_ALLOC_SIZE>();

#[derive(Encode, Decode, Debug, Clone)]
#[bincode(
    encode_bounds = "D: Direction",
    decode_bounds = "D: Direction, <D as Direction>::RecvType: bincode::Decode<__Context>",
    borrow_decode_bounds = "D: Direction, <D as Direction>::RecvType:  bincode::BorrowDecode<'__de, __Context>"
)]
pub enum ReceiveSerial<D: Direction> {
    MagicBytes(MagicBytes<D>),
    Message(D::RecvType),
    /// You can only send messages if you have the conch. Also devices should only do work if no one
    /// downstream of them has the conch.
    Conch,
    Reset,
    // to allow devices to ignore messages they don't understand
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
            ReceiveSerial::Conch => "Conch".into(),
            ReceiveSerial::Reset => "Reset".into(),
            _ => "Unused".into(),
        }
    }
}

/// A message sent from a coordinator
#[derive(Encode, Decode, Debug, Clone)]
pub struct CoordinatorSendMessage<B = CoordinatorSendBody> {
    pub target_destinations: Destination,
    pub message_body: B,
}

impl CoordinatorSendMessage {
    pub fn to(device_id: DeviceId, body: CoordinatorSendBody) -> Self {
        Self {
            target_destinations: Destination::Particular([device_id].into()),
            message_body: body,
        }
    }
}

#[cfg(feature = "coordinator")]
impl TryFrom<frostsnap_core::coordinator::CoordinatorSend>
    for CoordinatorSendMessage<CoordinatorSendBody>
{
    type Error = &'static str;

    fn try_from(value: frostsnap_core::coordinator::CoordinatorSend) -> Result<Self, Self::Error> {
        match value {
            frostsnap_core::coordinator::CoordinatorSend::ToDevice {
                message,
                destinations,
            } => Ok(CoordinatorSendMessage {
                target_destinations: Destination::from(destinations),
                message_body: CoordinatorSendBody::Core(message),
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

impl<B: Gist> Gist for CoordinatorSendMessage<B> {
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
    DataWipe,
}

impl From<CoordinatorSendBody> for WireCoordinatorSendBody {
    fn from(value: CoordinatorSendBody) -> Self {
        use CoordinatorSendBody::*;
        match value {
            AnnounceAck => WireCoordinatorSendBody::AnnounceAck,
            Cancel => WireCoordinatorSendBody::Cancel,
            Upgrade(upgrade) => WireCoordinatorSendBody::Upgrade(upgrade),
            _ => WireCoordinatorSendBody::EncapsV0(EncapsBody(
                bincode::encode_to_vec(value, BINCODE_CONFIG).expect("encoding is infallible"),
            )),
        }
    }
}

impl From<CoordinatorSendMessage> for CoordinatorSendMessage<WireCoordinatorSendBody> {
    fn from(value: CoordinatorSendMessage) -> Self {
        CoordinatorSendMessage {
            target_destinations: value.target_destinations,
            message_body: value.message_body.into(),
        }
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum CoordinatorUpgradeMessage {
    PrepareUpgrade {
        size: u32,
        firmware_digest: Sha256Digest,
    },
    EnterUpgradeMode,
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum NameCommand {
    Preview(String),
    Prompt(String),
}

impl Gist for CoordinatorSendBody {
    fn gist(&self) -> String {
        match self {
            CoordinatorSendBody::Core(core) => core.gist(),
            _ => format!("{self:?}"),
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

pub trait HasMagicBytes {
    const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN];
    const VERSION_SIGNAL: MagicBytesVersion;
}

pub trait Direction: HasMagicBytes {
    type RecvType: bincode::Encode + Gist;
    type Opposite: Direction;
}

impl HasMagicBytes for Upstream {
    const VERSION_SIGNAL: MagicBytesVersion = 0;
    const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = MAGICBYTES_RECV_UPSTREAM;
}

impl Direction for Upstream {
    type RecvType = CoordinatorSendMessage<WireCoordinatorSendBody>;
    type Opposite = Downstream;
}

impl HasMagicBytes for Downstream {
    const VERSION_SIGNAL: MagicBytesVersion = 1;
    const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = MAGICBYTES_RECV_DOWNSTREAM;
}

impl Direction for Downstream {
    type RecvType = DeviceSendMessage<WireDeviceSendBody>;
    type Opposite = Upstream;
}

impl<O: HasMagicBytes> bincode::Encode for MagicBytes<O> {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        let mut magic_bytes = O::MAGIC_BYTES;
        magic_bytes[magic_bytes.len() - 1] += O::VERSION_SIGNAL;
        encoder.writer().write(&magic_bytes)
    }
}

impl<Context, O: HasMagicBytes> bincode::Decode<Context> for MagicBytes<O> {
    fn decode<D: bincode::de::Decoder>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let mut bytes = [0u8; MAGIC_BYTES_LEN];
        decoder.reader().read(&mut bytes)?;
        let expected = O::MAGIC_BYTES;
        let except_version_signal_byte = ..MAGIC_BYTES_LEN - 1;
        if bytes[except_version_signal_byte] == expected[except_version_signal_byte] {
            // We don't care about version signal here yet
            Ok(MagicBytes(PhantomData))
        } else {
            Err(bincode::error::DecodeError::OtherString(format!(
                "was expecting magic bytes {:02x?} but got {:02x?}",
                O::MAGIC_BYTES,
                bytes
            )))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceSupportedFeatures {
    pub conch_enabled: bool,
}

impl DeviceSupportedFeatures {
    pub fn from_version(version: u8) -> Self {
        DeviceSupportedFeatures {
            conch_enabled: version >= 1,
        }
    }
}

impl<'de, Context, O: HasMagicBytes> bincode::BorrowDecode<'de, Context> for MagicBytes<O> {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(
        decoder: &mut D,
    ) -> core::result::Result<Self, bincode::error::DecodeError> {
        bincode::Decode::decode(decoder)
    }
}

/// Message sent from a device to the coordinator
#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub struct DeviceSendMessage<B> {
    pub from: DeviceId,
    pub body: B,
}

impl<B: Gist> Gist for DeviceSendMessage<B> {
    fn gist(&self) -> String {
        format!("{} <= {}", self.body.gist(), self.from)
    }
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum DeviceSendBody {
    Core(frostsnap_core::message::DeviceToCoordinatorMessage),
    Debug { message: String },
    Announce { firmware_digest: Sha256Digest },
    SetName { name: String },
    DisconnectDownstream,
    NeedName,
    _LegacyAckUpgradeMode, // Used by earliest devices
    Misc(CommsMisc),
}

#[derive(Encode, Decode, Debug, Clone)]
pub enum CommsMisc {
    /// A device has ack'd going into upgrade mode.
    AckUpgradeMode,
    /// Tells the coordinator that a device has confirmed to show it's backup.
    /// core doesn't provide a way to tell the coordinator that showing the backup was confirmed so
    /// we have this here.
    DisplayBackupConfrimed,
}

impl Gist for CommsMisc {
    fn gist(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Encode, Decode, Debug, Clone, PartialEq)]
pub enum WireDeviceSendBody {
    _Core,
    Debug { message: String },
    Announce { firmware_digest: Sha256Digest },
    SetName { name: String },
    DisconnectDownstream,
    NeedName,
    _LegacyAckUpgradeMode, // Used by earliest devices
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

impl From<DeviceSendMessage<DeviceSendBody>> for DeviceSendMessage<WireDeviceSendBody> {
    fn from(value: DeviceSendMessage<DeviceSendBody>) -> Self {
        DeviceSendMessage {
            from: value.from,
            body: value.body.into(),
        }
    }
}

impl WireDeviceSendBody {
    pub fn decode(self) -> Result<DeviceSendBody, bincode::error::DecodeError> {
        Ok(match self {
            WireDeviceSendBody::_Core => {
                return Err(bincode::error::DecodeError::Other(
                    "core messages should never be sent here",
                ));
            }
            WireDeviceSendBody::Debug { message } => DeviceSendBody::Debug { message },
            WireDeviceSendBody::Announce { firmware_digest } => {
                DeviceSendBody::Announce { firmware_digest }
            }
            WireDeviceSendBody::SetName { name } => DeviceSendBody::SetName { name },
            WireDeviceSendBody::DisconnectDownstream => DeviceSendBody::DisconnectDownstream,
            WireDeviceSendBody::NeedName => DeviceSendBody::NeedName,
            WireDeviceSendBody::_LegacyAckUpgradeMode => DeviceSendBody::_LegacyAckUpgradeMode,
            WireDeviceSendBody::EncapsV0(encaps) => {
                let (msg, _) = bincode::decode_from_slice(encaps.0.as_ref(), BINCODE_CONFIG)?;
                msg
            }
        })
    }
}

impl Gist for DeviceSendBody {
    fn gist(&self) -> String {
        match self {
            DeviceSendBody::Core(msg) => msg.gist(),
            DeviceSendBody::Debug { message } => format!("debug: {message}"),
            _ => format!("{self:?}"),
        }
    }
}

pub type MagicBytesVersion = u8;

pub fn make_progress_on_magic_bytes<D: Direction>(
    remaining: impl Iterator<Item = u8>,
    progress: usize,
) -> (usize, Option<MagicBytesVersion>) {
    let magic_bytes = D::MAGIC_BYTES;
    _make_progress_on_magic_bytes(remaining, &magic_bytes, progress)
}

fn _make_progress_on_magic_bytes(
    remaining: impl Iterator<Item = u8>,
    magic_bytes: &[u8],
    mut progress: usize,
) -> (usize, Option<MagicBytesVersion>) {
    for byte in remaining {
        // the last byte is used for version signaling -- doesn't need to be exact match
        let is_last_byte = progress == magic_bytes.len() - 1;
        let expected_byte = magic_bytes[progress];
        if is_last_byte && byte >= expected_byte {
            return (0, Some(byte - expected_byte));
        } else if byte == expected_byte {
            progress += 1;
        } else {
            progress = 0;
        }
    }

    (progress, None)
}

pub fn find_and_remove_magic_bytes<D: Direction>(buff: &mut Vec<u8>) -> Option<MagicBytesVersion> {
    let magic_bytes = D::MAGIC_BYTES;
    _find_and_remove_magic_bytes(buff, &magic_bytes[..])
}

fn _find_and_remove_magic_bytes(
    buff: &mut Vec<u8>,
    magic_bytes: &[u8],
) -> Option<MagicBytesVersion> {
    let mut consumed = 0;
    let (_, found) = _make_progress_on_magic_bytes(
        buff.iter().cloned().inspect(|_| consumed += 1),
        magic_bytes,
        0,
    );

    if found.is_some() {
        *buff = buff.split_off(consumed);
    }

    found
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Sha256Digest(pub [u8; 32]);

frostsnap_core::impl_display_debug_serialize! {
    fn to_bytes(digest: &Sha256Digest) -> [u8;32] {
        digest.0
    }
}

frostsnap_core::impl_fromstr_deserialize! {
    name => "sha256 digest",
    fn from_bytes(bytes: [u8;32]) -> Sha256Digest {
        Sha256Digest(bytes)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn remove_magic_bytes() {
        let mut bytes = b"hello world".to_vec();
        assert_eq!(_find_and_remove_magic_bytes(&mut bytes, b"magic"), None);

        let mut bytes = b"hello magic world".to_vec();

        assert_eq!(_find_and_remove_magic_bytes(&mut bytes, b"magic"), Some(0));
        assert_eq!(bytes, b" world");

        let mut bytes = b"hello magicmagic world".to_vec();
        assert_eq!(_find_and_remove_magic_bytes(&mut bytes, b"magic"), Some(0));
        assert_eq!(bytes, b"magic world");

        let mut bytes = b"magic".to_vec();
        assert_eq!(_find_and_remove_magic_bytes(&mut bytes, b"magic"), Some(0));
        assert_eq!(bytes, b"");

        let mut bytes = b"hello magid world".to_vec();
        assert_eq!(_find_and_remove_magic_bytes(&mut bytes, b"magic"), Some(1));

        let mut bytes = b"hello magif world".to_vec();
        assert_eq!(_find_and_remove_magic_bytes(&mut bytes, b"magic"), Some(3));
    }
}
