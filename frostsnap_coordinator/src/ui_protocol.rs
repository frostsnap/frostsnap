use std::any::Any;

use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{message::CoordinatorToUserMessage, DeviceId};

/// A UiProtocol is a layer between the protocol the devices and the coordinator are executing e.g.
/// keygen, signing etc and the actual UI. Applications intercept frostsnap_core's
/// [`CoordinatorToUserMessage`] and send them to `UiProtocol` which usually has a channel (not
/// represented in the API) to the UI where it outputs states. Usually this channel is passed in as
/// a `Sink` into the constructor.
///
/// With the `poll` method it can communicate to the devices or the coordinator's storage.
pub trait UiProtocol: Send + Any + 'static {
    fn name(&self) -> &'static str {
        core::any::type_name_of_val(self)
    }
    fn cancel(&mut self);
    fn is_complete(&self) -> Option<Completion>;
    fn connected(&mut self, _id: DeviceId) {}
    fn disconnected(&mut self, id: DeviceId);
    fn process_to_user_message(&mut self, _message: CoordinatorToUserMessage) {}
    fn process_upgrade_mode_ack(&mut self, _from: DeviceId) {}
    /// `poll` allows the UiProtocol to communicate to the rest of the system.  The reason the ui protocol needs
    /// to do this is subtle: core messages may need to be sent out only when a device is next
    /// connected. The UI protocol is currently the point that manages the effect of device
    /// connections and disconnections on the protocol so it is able to violate boundries here a bit
    /// and send out core messages.
    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>);

    fn as_any(&self) -> &dyn Any;
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

#[derive(Clone, Debug)]
pub enum Completion {
    Success,
    Abort { send_cancel_to_all_devices: bool },
}

#[derive(Clone, Debug)]
pub enum UiToStorageMessage {
    /// Clear the signing session. Note that the signing session is stored by core but the
    /// application is left to decide when to clear it.
    ClearSigningSession,
}
