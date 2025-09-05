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
    pub device_to_share_index: BTreeMap<DeviceId, NonZeroU32>,
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
    pub fn new_with_id(
        devices: BTreeSet<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        coordinator_public_key: Point,
        keygen_id: KeygenId,
    ) -> Self {
        let device_to_share_index: BTreeMap<_, _> = devices
            .iter()
            .enumerate()
            .map(|(index, device_id)| {
                (
                    *device_id,
                    NonZeroU32::new((index + 1) as u32).expect("we added one"),
                )
            })
            .collect();

        Self {
            device_to_share_index,
            threshold,
            key_name,
            purpose,
            keygen_id,
            coordinator_public_key,
        }
    }
    pub fn new(
        devices: BTreeSet<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        coordinator_public_key: Point,
        rng: &mut impl rand_core::RngCore, // for the keygen id
    ) -> Self {
        let mut id = [0u8; 16];
        rng.fill_bytes(&mut id[..]);

        Self::new_with_id(
            devices,
            threshold,
            key_name,
            purpose,
            coordinator_public_key,
            KeygenId::from_bytes(id),
        )
    }

    pub fn devices(&self) -> BTreeSet<DeviceId> {
        self.device_to_share_index.keys().cloned().collect()
    }
}
