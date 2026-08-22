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
/// The error is encoded the way frb encodes one for an ordinary call, so Dart decodes it into the
/// same `AnyhowException` it would have received had the `Result` reached it.
///
/// `T: Clone` is frb's, not ours: it derives `Clone` on a sink that holds `T` only in a
/// `PhantomData`, and the derive asks for it anyway.
pub fn report_start_failure<T: SseEncode + Clone, R>(
    sink: StreamSink<T>,
    start: impl FnOnce(SinkWrap<T>) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    let error_sink = sink.clone();
    start(SinkWrap(sink)).inspect_err(|e| {
        let _ = error_sink.add_error(format!("{e:?}"));
    })
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
