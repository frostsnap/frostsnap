use super::*;

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Kind)]
pub enum Keygen {
    Begin(Begin),
    Check {
        keygen_id: KeygenId,
        agg_input: encpedpop::AggKeygenInput,
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
        devices: Vec<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        keygen_id: KeygenId,
    ) -> Self {
        Self {
            devices,
            threshold,
            key_name,
            purpose,
            keygen_id,
        }
    }

    pub fn new(
        devices: Vec<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        rng: &mut impl rand_core::RngCore, // for the keygen id
    ) -> Self {
        let mut id = [0u8; 16];
        rng.fill_bytes(&mut id[..]);

        Self::new_with_id(
            devices,
            threshold,
            key_name,
            purpose,
            KeygenId::from_bytes(id),
        )
    }

    /// Get the devices as a BTreeSet
    pub fn device_set(&self) -> BTreeSet<DeviceId> {
        self.devices.iter().cloned().collect()
    }

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
