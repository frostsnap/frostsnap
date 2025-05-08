use alloc::{string::String, vec::Vec};
use crate::{Direction, HasMagicBytes, MAGIC_BYTES_LEN, MagicBytesVersion};
use frostsnap_core::Gist;

#[derive(Debug, Clone)]
pub struct FactoryUpstream;
#[derive(Debug, Clone)]
pub struct FactoryDownstream;

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum DeviceFactorySend {
    InitEntropyOk,
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum FactorySend {
    InitEntropy([u8; 32]),
    SetEsp32DsKey(Esp32DsKey),
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub struct Esp32DsKey {
    pub encrypted_params: Vec<u8>,
    pub hmac_key: [u8; 32],
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
