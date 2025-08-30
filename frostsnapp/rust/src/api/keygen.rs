use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::Result;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::keygen::KeyGenState;
use frostsnap_core::{device::KeyPurpose, AccessStructureRef, DeviceId, KeygenId, SessionHash};

#[frb(mirror(KeyGenState), unignore)]
pub struct _KeyGenState {
    pub threshold: usize,
    pub devices: Vec<DeviceId>, // not a set for frb compat
    pub got_shares: Vec<DeviceId>,
    pub all_shares: bool,
    pub session_acks: Vec<DeviceId>,
    pub all_acks: bool,
    pub session_hash: Option<SessionHash>,
    pub finished: Option<AccessStructureRef>,
    pub aborted: Option<String>,
    pub keygen_id: KeygenId,
}

impl super::coordinator::Coordinator {
    pub fn generate_new_key(
        &self,
        threshold: u16,
        devices: Vec<DeviceId>,
        key_name: String,
        network: BitcoinNetwork,
        event_stream: StreamSink<KeyGenState>,
    ) -> Result<()> {
        self.0.generate_new_key(
            devices,
            threshold,
            key_name,
            KeyPurpose::Bitcoin(network),
            SinkWrap(event_stream),
        )
    }

    pub fn finalize_keygen(&self, keygen_id: KeygenId) -> Result<AccessStructureRef> {
        self.0.finalize_keygen(keygen_id, crate::TEMP_KEY)
    }
}
