pub use crate::api::KeyPurpose;
use crate::sink_wrap::SinkWrap;
use anyhow::Result;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::erase_device::EraseDeviceState;
pub use frostsnap_core::coordinator::restoration::RestorationState;
pub use frostsnap_core::coordinator::CoordAccessStructure as AccessStructure;
use frostsnap_core::{
    coordinator::CoordFrostKey,
    schnorr_fun::frost::{ShareIndex, SharedKey},
    tweak::Xpub,
    AccessStructureId, AccessStructureRef, DeviceId, KeyId, MasterAppkey,
};
use std::collections::BTreeMap;
use tracing::{event, Level};

use crate::{coordinator::FfiCoordinator, frb_generated::StreamSink};

pub use super::backup_run::{BackupDevice, BackupRun};

#[frb(mirror(EraseDeviceState), non_opaque)]
pub enum _EraseDeviceState {
    WaitingForConfirmation,
    Confirmed,
}

#[derive(Clone, Debug)]
pub struct KeyState {
    pub keys: Vec<FrostKey>,
    pub restoring: Vec<RestorationState>,
}

#[derive(Clone, Debug)]
pub struct FrostKey(pub(crate) frostsnap_core::coordinator::CoordFrostKey);

impl FrostKey {
    #[frb(sync)]
    pub fn master_appkey(&self) -> MasterAppkey {
        self.0.complete_key.master_appkey
    }

    #[frb(sync)]
    pub fn key_id(&self) -> KeyId {
        self.0.key_id
    }

    #[frb(sync)]
    pub fn key_name(&self) -> String {
        self.0.key_name.to_string()
    }

    #[frb(sync)]
    pub fn access_structures(&self) -> Vec<AccessStructure> {
        self.0
            .complete_key
            .access_structures
            .values()
            .cloned()
            .collect()
    }

    #[frb(sync)]
    pub fn get_access_structure(
        &self,
        access_structure_id: AccessStructureId,
    ) -> Option<AccessStructure> {
        self.0
            .complete_key
            .access_structures
            .get(&access_structure_id)
            .cloned()
            .map(From::from)
    }

    #[frb(sync)]
    pub fn bitcoin_network(&self) -> Option<BitcoinNetwork> {
        self.0.purpose.bitcoin_network()
    }
}

impl From<CoordFrostKey> for FrostKey {
    fn from(value: CoordFrostKey) -> Self {
        FrostKey(value)
    }
}

// this is here just so we can extend it
#[frb(mirror(AccessStructure))]
#[allow(unused)]
pub struct _AccessStructure {
    app_shared_key: Xpub<SharedKey>,
    device_to_share_index: BTreeMap<DeviceId, ShareIndex>,
}

#[frb(external)]
impl AccessStructure {
    #[frb(sync)]
    pub fn threshold(&self) -> u16 {}

    #[frb(sync)]
    pub fn access_structure_ref(&self) -> AccessStructureRef {}

    #[frb(sync)]
    pub fn access_structure_id(&self) -> AccessStructureId {}

    #[frb(sync)]
    pub fn master_appkey(&self) -> MasterAppkey {}
}

pub trait AccessStructureExt {
    #[frb(sync)]
    fn frb_override_devices(&self) -> Vec<DeviceId>;

    #[frb(sync)]
    fn devices_by_share_index(&self) -> Vec<DeviceId>;

    #[frb(sync)]
    fn short_id(&self) -> String;

    #[frb(sync)]
    fn get_device_short_share_index(&self, device_id: DeviceId) -> Option<u32>;
}

impl AccessStructureExt for AccessStructure {
    #[frb(sync)]
    fn short_id(&self) -> String {
        self.access_structure_id().to_string().split_off(8)
    }

    #[frb(sync)]
    fn frb_override_devices(&self) -> Vec<DeviceId> {
        self.devices().collect()
    }

    #[frb(sync)]
    fn devices_by_share_index(&self) -> Vec<DeviceId> {
        AccessStructure::devices_by_share_index(self)
    }

    #[frb(sync)]
    fn get_device_short_share_index(&self, device_id: DeviceId) -> Option<u32> {
        use core::convert::TryFrom;
        self.device_to_share_indicies()
            .get(&device_id)
            .and_then(|share_index| u32::try_from(*share_index).ok())
    }
}

pub struct Coordinator(pub(crate) FfiCoordinator);

impl Coordinator {
    pub fn start_thread(&self) -> Result<()> {
        self.0.start()
    }

    pub fn update_name_preview(&self, id: DeviceId, name: String) -> Result<()> {
        self.0.update_name_preview(id, &name)
    }

    pub fn finish_naming(&self, id: DeviceId, name: String) -> Result<()> {
        self.0.finish_naming(id, &name)
    }

    pub fn send_cancel(&self, id: DeviceId) {
        event!(Level::WARN, "dart sent cancel");
        self.0.send_cancel(id);
    }

    pub fn send_cancel_all(&self) {
        event!(Level::WARN, "dart sent cancel all");
        self.0.usb_sender.send_cancel_all();
    }

    #[frb(sync)]
    pub fn key_state(&self) -> KeyState {
        self.0.key_state()
    }

    pub fn sub_key_events(&self, stream: StreamSink<KeyState>) -> Result<()> {
        self.0.sub_key_events(SinkWrap(stream));
        Ok(())
    }

    #[frb(sync)]
    pub fn access_structures_involving_device(
        &self,
        device_id: DeviceId,
    ) -> Vec<AccessStructureRef> {
        self.0
            .frost_keys()
            .into_iter()
            .flat_map(|frost_key| {
                frost_key
                    .access_structures()
                    .filter(|access_structure| access_structure.contains_device(device_id))
                    .map(|access_structure| access_structure.access_structure_ref())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[frb(sync)]
    pub fn frost_keys_involving_device(&self, device_id: DeviceId) -> Vec<FrostKey> {
        self.0
            .frost_keys()
            .into_iter()
            .filter(|coord_frost_key| {
                coord_frost_key
                    .access_structures()
                    .any(|coord_access_structure| coord_access_structure.contains_device(device_id))
            })
            .map(FrostKey::from)
            .collect()
    }

    #[frb(sync)]
    pub fn get_frost_key(&self, key_id: KeyId) -> Option<FrostKey> {
        self.0.get_frost_key(key_id).map(FrostKey::from)
    }

    #[frb(sync)]
    pub fn get_access_structure(&self, as_ref: AccessStructureRef) -> Option<AccessStructure> {
        self.0.get_access_structure(as_ref)
    }

    pub fn delete_key(&self, key_id: KeyId) -> Result<()> {
        self.0.delete_key(key_id)
    }

    pub fn delete_share(
        &self,
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
    ) -> Result<()> {
        self.0.delete_share(access_structure_ref, device_id)
    }

    pub fn erase_device(&self, device_id: DeviceId, sink: StreamSink<EraseDeviceState>) {
        self.0.erase_device(device_id, SinkWrap(sink));
    }

    pub fn erase_all_devices(&self) {
        self.0.erase_all_devices();
    }

    pub fn cancel_protocol(&self) {
        self.0.cancel_protocol()
    }

    #[frb(sync)]
    pub fn get_device_name(&self, id: DeviceId) -> Option<String> {
        self.0.get_device_name(id)
    }
}
