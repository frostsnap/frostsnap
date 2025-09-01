use crate::sink_wrap::SinkWrap;
use anyhow::Result;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
pub use frostsnap_core::coordinator::CoordAccessStructure as AccessStructure;
pub use frostsnap_core::device::KeyPurpose;
use frostsnap_core::{
    coordinator::CoordFrostKey,
    schnorr_fun::frost::{ShareIndex, SharedKey},
    tweak::Xpub,
    AccessStructureId, AccessStructureRef, DeviceId, KeyId, MasterAppkey, SymmetricKey,
};
use std::collections::BTreeMap;
use tracing::{event, Level};

use crate::{coordinator::FfiCoordinator, frb_generated::StreamSink};
const TEMP_KEY: SymmetricKey = SymmetricKey([42u8; 32]);

#[derive(Clone, Debug)]
pub struct KeyState {
    pub keys: Vec<FrostKey>,
    pub restoring: Vec<crate::api::recovery::RestoringKey>,
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
        self.0.key_name.clone()
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
    fn short_id(&self) -> String;
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
}

pub struct Coordinator(pub(crate) FfiCoordinator);

#[frb(external)]
impl KeyPurpose {
    #[frb(sync)]
    pub fn bitcoin_network(&self) -> Option<BitcoinNetwork> {}
}

impl Coordinator {
    pub fn start_thread(&self) -> Result<()> {
        self.0.start()
    }

    pub fn update_name_preview(&self, id: DeviceId, name: String) {
        self.0.update_name_preview(id, &name);
    }

    pub fn finish_naming(&self, id: DeviceId, name: String) {
        self.0.finish_naming(id, &name)
    }

    pub fn send_cancel(&self, id: DeviceId) {
        event!(Level::WARN, "dart sent cancel");
        self.0.send_cancel(id);
    }

    pub fn display_backup(
        &self,
        id: DeviceId,
        access_structure_ref: AccessStructureRef,
        sink: StreamSink<bool>,
    ) -> Result<()> {
        self.0
            .request_display_backup(id, access_structure_ref, TEMP_KEY, SinkWrap(sink))?;
        Ok(())
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

    pub fn wipe_device_data(&self, device_id: DeviceId) {
        self.0.wipe_device_data(device_id);
    }

    pub fn wipe_all_devices(&self) {
        self.0.wipe_all_devices();
    }

    pub fn cancel_protocol(&self) {
        self.0.cancel_protocol()
    }

    #[frb(sync)]
    pub fn get_device_name(&self, id: DeviceId) -> Option<String> {
        self.0.get_device_name(id)
    }
}
