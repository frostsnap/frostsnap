use crate::{Direction, HasMagicBytes, MagicBytesVersion, MAGIC_BYTES_LEN};
use alloc::{string::String, vec::Vec};
use frostsnap_core::{
    schnorr_fun::{
        fun::{marker::EvenY, Point},
        Signature,
    },
    Gist,
};

pub const DS_KEY_SIZE_BITS: usize = 3072;
pub const DS_KEY_SIZE_BYTES: usize = DS_KEY_SIZE_BITS / 8;

pub fn pad_message_for_rsa(message_digest: &[u8]) -> [u8; DS_KEY_SIZE_BYTES] {
    // Hard-code the ASN.1 DigestInfo prefix for SHA-256
    const SHA256_ASN1_PREFIX: &[u8] = &[
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];

    let mut padded_block = [0; DS_KEY_SIZE_BYTES];

    // PKCS#1 v1.5 format: 0x00 || 0x01 || PS || 0x00 || T
    padded_block[0] = 0x00;
    padded_block[1] = 0x01;

    // Calculate padding length
    let padding_len = DS_KEY_SIZE_BYTES - SHA256_ASN1_PREFIX.len() - message_digest.len() - 3;

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
    SendState { rsa_pub_key: Option<Vec<u8>> },
    InitEntropyOk,
    ReceivedDsKey,
    PresentGenuineCertificate(Certificate),
    SignedChallenge { signature: [u8; 384] },
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub enum FactorySend {
    CheckState,
    InitEntropy([u8; 32]),
    SetEsp32DsKey(Esp32DsKey),
    SetGenuineCertificate(Certificate),
    RequestCertificate,
    Challenge(Vec<u8>),
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub struct Esp32DsKey {
    pub encrypted_params: Vec<u8>,
    pub ds_hmac_key: [u8; 32],
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone, PartialEq)]
pub struct Certificate {
    pub rsa_key: Vec<u8>,
    pub serial_number: u32,
    pub timestamp: u64,
    pub case_color: String,
    pub signature: Signature,
    pub factory_key: Point<EvenY>,
}

impl Gist for DeviceFactorySend {
    fn gist(&self) -> String {
        match self {
            DeviceFactorySend::SendState { .. } => "SendState",
            DeviceFactorySend::InitEntropyOk => "InitEntropyOk",
            DeviceFactorySend::ReceivedDsKey { .. } => "SetDs",
            DeviceFactorySend::PresentGenuineCertificate(_) => "SavedGenuineCertificate",
            DeviceFactorySend::SignedChallenge { .. } => "SignedChallenge",
        }
        .into()
    }
}

impl Gist for FactorySend {
    fn gist(&self) -> String {
        match self {
            FactorySend::CheckState => "CheckState",
            FactorySend::SetEsp32DsKey { .. } => "SetEsp32DsKey",
            FactorySend::InitEntropy(_) => "InitEntropy",
            FactorySend::SetGenuineCertificate(_) => "GenuineCertificate",
            FactorySend::RequestCertificate => "RequestCertificate",
            FactorySend::Challenge(_) => "Challenge",
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
