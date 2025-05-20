use crate::{Direction, HasMagicBytes, MagicBytesVersion, MAGIC_BYTES_LEN};
use alloc::{string::String, vec::Vec};
use frostsnap_core::{schnorr_fun::Signature, Gist};

pub const ETS_DS_MAX_BITS: usize = 3072;
pub const REPRODUCING_TEST_VECTORS: bool = false;

pub fn pad_message_for_rsa(message_digest: &[u8]) -> Vec<u8> {
    // Hard-code the ASN.1 DigestInfo prefix for SHA-256
    const SHA256_ASN1_PREFIX: &[u8] = &[
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];

    // Convert bits to bytes (rounding up)
    let key_size_bytes = (ETS_DS_MAX_BITS + 7) / 8;

    let mut padded_block = vec![0; key_size_bytes];

    // PKCS#1 v1.5 format: 0x00 || 0x01 || PS || 0x00 || T
    padded_block[0] = 0x00;
    padded_block[1] = 0x01;

    // Calculate padding length
    let padding_len = key_size_bytes - SHA256_ASN1_PREFIX.len() - message_digest.len() - 3;

    // Fill with 0xFF bytes
    for i in 0..padding_len {
        padded_block[2 + i] = 0xFF;
    }

    // Add 0x00 separator
    padded_block[2 + padding_len] = 0x00;

    // Add prefix (ASN.1 DigestInfo)
    let prefix_offset = 3 + padding_len;
    padded_block[prefix_offset..(prefix_offset + SHA256_ASN1_PREFIX.len())]
        .copy_from_slice(SHA256_ASN1_PREFIX);

    // Add message digest
    let digest_offset = prefix_offset + SHA256_ASN1_PREFIX.len();
    padded_block[digest_offset..(digest_offset + message_digest.len())]
        .copy_from_slice(message_digest);

    padded_block
}

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
