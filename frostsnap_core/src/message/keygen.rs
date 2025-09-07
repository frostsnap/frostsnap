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
    pub coordinator_public_key: Point,
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

impl Begin {
    /// Generate the device to share index mapping based on the device order in the Vec
    pub fn device_to_share_index(&self) -> BTreeMap<DeviceId, core::num::NonZeroU32> {
        self.devices
            .iter()
            .enumerate()
            .map(|(index, device_id)| {
                (
                    *device_id,
                    core::num::NonZeroU32::new((index as u32) + 1).expect("we added one"),
                )
            })
            .collect()
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
