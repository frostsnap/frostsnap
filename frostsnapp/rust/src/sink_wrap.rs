use crate::{
    api::{coordinator::KeyState, device_list::DeviceListUpdate},
    frb_generated::StreamSink,
};
use frostsnap_coordinator::{
    // bitcoin::chain_sync::ChainStatus,
    bitcoin::chain_sync::ChainStatus,
    firmware_upgrade::FirmwareUpgradeConfirmState,
    keygen::KeyGenState,
    signing::SigningState,
    verify_address::VerifyAddressProtocolState,
};

// we need to wrap it so we can impl it on foreign FRB type. You can't do a single generic impl. Try
// it if you don't believe me.
pub struct SinkWrap<T>(pub StreamSink<T>);

macro_rules! bridge_sink {
    ($type:ty) => {
        impl<A: Into<$type> + Send + 'static> frostsnap_coordinator::Sink<A> for SinkWrap<$type> {
            fn send(&self, state: A) {
                let _ = self.0.add(state.into());
            }
        }
    };
}

bridge_sink!(KeyGenState);
bridge_sink!(FirmwareUpgradeConfirmState);
bridge_sink!(VerifyAddressProtocolState);
bridge_sink!(SigningState);
bridge_sink!(bool);
bridge_sink!(f32);
bridge_sink!(ChainStatus);
bridge_sink!(DeviceListUpdate);
bridge_sink!(KeyState);
bridge_sink!(());
bridge_sink!(crate::api::recovery::EnterPhysicalBackupState);
bridge_sink!(crate::api::recovery::WaitForRecoveryShareState);
