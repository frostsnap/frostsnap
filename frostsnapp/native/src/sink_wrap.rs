use flutter_rust_bridge::StreamSink;
use frostsnap_coordinator::{
    bitcoin::chain_sync::ChainStatus, check_share::CheckShareState,
    firmware_upgrade::FirmwareUpgradeConfirmState, keygen::KeyGenState, signing::SigningState,
};

// we need to wrap it so we can impl it on foreign FRB type. You can't do a single generic impl. Try
// it if you don't believe me.
pub struct SinkWrap<T>(pub StreamSink<T>);

macro_rules! bridge_sink {
    ($type:ty) => {
        impl frostsnap_coordinator::Sink<$type> for SinkWrap<$type> {
            fn send(&self, state: $type) {
                self.0.add(state);
            }

            fn close(&self) {
                self.0.close();
            }
        }
    };
}

bridge_sink!(KeyGenState);
bridge_sink!(FirmwareUpgradeConfirmState);
bridge_sink!(SigningState);
bridge_sink!(CheckShareState);
bridge_sink!(bool);
bridge_sink!(ChainStatus);
