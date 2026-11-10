use crate::{
    api::{coordinator::KeyState, device_list::DeviceListUpdate},
    frb_generated::{SseEncode, StreamSink},
};
use frostsnap_coordinator::{
    // bitcoin::chain_sync::ChainStatus,
    bitcoin::chain_sync::ChainStatus,
    erase_device::EraseDeviceState,
    firmware_upgrade::FirmwareUpgradeConfirmState,
    keygen::KeyGenState,
    nonce_replenish::NonceReplenishState,
    signing::SigningState,
    verify_address::VerifyAddressProtocolState,
};

// we need to wrap it so we can impl it on foreign FRB type. You can't do a single generic impl. Try
// it if you don't believe me.
pub struct SinkWrap<T>(pub StreamSink<T>);

/// Runs something that consumes `sink`, putting a failure to start onto that same stream.
///
/// frb compiles a stream-returning function into a fire-and-forget call: it drops the `Result`
/// rather than handing it back, so an error returned from one of them reaches nobody and the
/// screen goes on waiting for a session that was never created. A `StreamSink` is a handle to a
/// Dart port and clones for free, so the error path keeps one and the failure arrives where the
/// screen is already listening.
///
/// Once the error is on the stream this returns `Ok(())`: the dropped `Result` would otherwise
/// surface the same failure a second time as an unhandled zone error. Only a failure to deliver
/// is returned.
///
/// `T: Clone` is frb's, not ours: it derives `Clone` on a sink that holds `T` only in a
/// `PhantomData`, and the derive asks for it anyway.
pub fn report_start_failure<T: SseEncode + Clone>(
    sink: StreamSink<T>,
    start: impl FnOnce(SinkWrap<T>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let error_sink = sink.clone();
    match start(SinkWrap(sink)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = error.to_string();
            error_sink.add_error(error).map_err(|send_error| {
                anyhow::anyhow!("failed to deliver stream start error ({send_error}): {message}")
            })?;
            Ok(())
        }
    }
}

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
bridge_sink!(NonceReplenishState);
bridge_sink!(());
bridge_sink!(crate::api::backup_run::BackupRun);
bridge_sink!(crate::api::backup_run::DisplayBackupState);
bridge_sink!(crate::api::recovery::EnterPhysicalBackupState);
bridge_sink!(crate::api::recovery::WaitForSingleDeviceState);
bridge_sink!(EraseDeviceState);
bridge_sink!(crate::api::recovery::CheckBackupState);
bridge_sink!(crate::api::verifiable_card::CardFlowState);
