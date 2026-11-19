use schnorr_fun::frost::chilldkg::certpedpop::vrf_cert;

use super::*;

pub const VRF_CERT_SCHEME_ID: &str = "cert-vrf-v0";

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum Keygen {
    Begin(Begin),
    CertifyPlease {
        keygen_id: KeygenId,
        agg_input: certpedpop::AggKeygenInput,
    },
    Check {
        keygen_id: KeygenId,
        certificate: BTreeMap<Point, vrf_cert::CertVrfProof>,
    },
    /// Actually save key to device.
    Finalize {
        keygen_id: KeygenId,
    },
    /// `Begin` for a key whose shares get verifiable backup cards. Firmware that doesn't know
    /// it drops it, so the app sends it only to devices that support cards.
    Begin2(Begin2),
}

impl From<Keygen> for CoordinatorToDeviceMessage {
    fn from(value: Keygen) -> Self {
        Self::KeyGen(value)
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct Begin {
    pub keygen_id: KeygenId,
    pub devices: Vec<DeviceId>,
    pub threshold: u16,
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub coordinator_public_keys: Vec<Point>,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct Begin2 {
    pub begin: Begin,
    /// Each share gets a 32-word verifiable card instead of a 25-word backup.
    pub card: bool,
}

impl From<Begin> for Keygen {
    fn from(value: Begin) -> Self {
        Self::Begin(value)
    }
}

impl From<Begin> for CoordinatorToDeviceMessage {
    fn from(value: Begin) -> Self {
        CoordinatorToDeviceMessage::KeyGen(value.into())
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum DeviceKeygen {
    Response(super::KeyGenResponse),
    Certify {
        keygen_id: KeygenId,
        vrf_cert: vrf_cert::CertVrfProof,
    },
    Ack(super::KeyGenAck),
}

impl From<DeviceKeygen> for DeviceToCoordinatorMessage {
    fn from(value: DeviceKeygen) -> Self {
        Self::KeyGen(value)
    }
}
