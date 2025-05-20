use crate::{Direction, HasMagicBytes, MagicBytesVersion, MAGIC_BYTES_LEN};
use alloc::{string::String, vec::Vec};
use frostsnap_core::{schnorr_fun::Signature, Gist};

pub const REPRODUCING_TEST_VECTORS: bool = false;

#[derive(Debug, Clone)]
pub struct FactoryUpstream;
#[derive(Debug, Clone)]
pub struct FactoryDownstream;

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum DeviceFactorySend {
    InitEntropyOk,
    SetDs { signature: Vec<u8> },
    SavedGenuineCertificate,
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum FactorySend {
    InitEntropy([u8; 32]),
    SetEsp32DsKey(Esp32DsKey),
    SetGenuineCertificate(GenuineCheckKey),
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub struct Esp32DsKey {
    pub encrypted_params: Vec<u8>,
    pub hmac_key: [u8; 32],
    pub challenge: Vec<u8>,
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub struct GenuineCheckKey {
    pub genuine_key: [u8; 32],
    pub certificate: Signature,
}

impl Gist for DeviceFactorySend {
    fn gist(&self) -> String {
        todo!()
    }
}

impl Gist for FactorySend {
    fn gist(&self) -> String {
        match self {
            FactorySend::SetEsp32DsKey { .. } => "SetEsp32DsKey",
            FactorySend::InitEntropy(_) => "InitEntropy",
            FactorySend::SetGenuineCertificate(_) => "GenuineCertificate",
        }
        .into()
    }
}

impl HasMagicBytes for FactoryUpstream {
    const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = *b"factup0";
    const VERSION_SIGNAL: MagicBytesVersion = 0;
}

impl Direction for FactoryUpstream {
    type RecvType = FactorySend;
    type Opposite = FactoryDownstream;
}

impl Direction for FactoryDownstream {
    type RecvType = DeviceFactorySend;
    type Opposite = FactoryUpstream;
}

impl HasMagicBytes for FactoryDownstream {
    const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = *b"factdn0";
    const VERSION_SIGNAL: MagicBytesVersion = 0;
}
