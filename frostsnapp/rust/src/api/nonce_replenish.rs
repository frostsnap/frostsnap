use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::Result;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::nonce_replenish::NonceReplenishState;
use frostsnap_core::DeviceId;

#[frb(mirror(NonceReplenishState), unignore)]
pub struct _NonceReplenishState {
    pub received_from: Vec<DeviceId>,
    pub devices: Vec<DeviceId>,
    pub abort: bool,
}

impl super::coordinator::Coordinator {
    pub fn replenish_nonces(
        &self,
        devices: Vec<DeviceId>,
        event_stream: StreamSink<NonceReplenishState>,
    ) -> Result<()> {
        self.0
            .replenish_nonces(devices.into_iter().collect(), SinkWrap(event_stream))?;

        Ok(())
    }
}
