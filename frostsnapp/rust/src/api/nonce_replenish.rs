use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::Result;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::nonce_replenish::NonceReplenishState;
use frostsnap_core::{coordinator::NonceReplenishRequest, DeviceId};

#[frb(mirror(NonceReplenishState), unignore)]
pub struct _NonceReplenishState {
    pub received_from: Vec<DeviceId>,
    pub devices: Vec<DeviceId>,
    pub abort: bool,
}

impl super::coordinator::Coordinator {
    #[frb(sync)]
    pub fn create_nonce_request(&self, devices: Vec<DeviceId>) -> NonceRequest {
        NonceRequest {
            inner: self
                .0
                .nonce_replenish_request(devices.into_iter().collect()),
        }
    }

    pub fn replenish_nonces(
        &self,
        nonce_request: NonceRequest,
        devices: Vec<DeviceId>,
        event_stream: StreamSink<NonceReplenishState>,
    ) -> Result<()> {
        self.0.replenish_nonces(
            nonce_request.inner,
            devices.into_iter().collect(),
            SinkWrap(event_stream),
        )
    }
}

pub struct NonceRequest {
    inner: NonceReplenishRequest,
}

impl NonceRequest {
    #[frb(sync)]
    pub fn some_nonces_requested(&self) -> bool {
        self.inner.some_nonces_requested()
    }
}
