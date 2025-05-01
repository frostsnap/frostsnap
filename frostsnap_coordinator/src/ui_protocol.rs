use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{coordinator::CoordinatorToUserMessage, DeviceId, Gist as _};
use std::any::Any;
use tracing::{event, Level};

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
    fn cancel(&mut self) {}
    fn is_complete(&self) -> Option<Completion>;
    fn connected(&mut self, _id: DeviceId, _state: DeviceMode) {}
    fn disconnected(&mut self, _id: DeviceId) {}
    fn process_to_user_message(&mut self, _message: CoordinatorToUserMessage) -> bool {
        false
    }
    fn process_comms_message(
        &mut self,
        _from: DeviceId,
        _message: frostsnap_comms::CommsMisc,
    ) -> bool {
        false
    }
    /// `poll` allows the UiProtocol to communicate to the rest of the system.  The reason the ui protocol needs
    /// to do this is subtle: core messages may need to be sent out only when a device is next
    /// connected. The UI protocol is currently the point that manages the effect of device
    /// connections and disconnections on the protocol so it is able to violate boundries here a bit
    /// and send out core messages.
    fn poll(&mut self) -> Vec<CoordinatorSendMessage>;

    fn as_any(&self) -> &dyn Any;
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

#[derive(Clone, Debug)]
pub enum Completion {
    Success,
    Abort { send_cancel_to_all_devices: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub enum DeviceMode {
    Blank,
    Recovery,
    Ready,
}

#[derive(Default)]
pub struct UiStack {
    protocols: Vec<Box<dyn UiProtocol>>,
}

impl UiStack {
    pub fn get_mut<T: UiProtocol>(&mut self) -> Option<&mut T> {
        for protocol in self.protocols.iter_mut().rev() {
            if let Some(found) = protocol.as_mut().as_mut_any().downcast_mut::<T>() {
                return Some(found);
            }
        }
        None
    }

    pub fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) {
        let mut found = false;
        for protocol in self.protocols.iter_mut().rev() {
            found = protocol.process_to_user_message(message.clone());
            if found {
                break;
            }
        }

        if !found {
            event!(
                Level::WARN,
                gist = message.gist(),
                "got unexpected to user message"
            );
        }
    }

    pub fn process_comms_message(&mut self, from: DeviceId, message: frostsnap_comms::CommsMisc) {
        let mut found = false;
        for protocol in self.protocols.iter_mut().rev() {
            found = protocol.process_comms_message(from, message.clone());
            if found {
                break;
            }
        }

        if !found {
            event!(
                Level::WARN,
                gist = message.gist(),
                "got unexpected comms message"
            );
        }
    }

    pub fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        let mut out = vec![];

        for protocol in self.protocols.iter_mut().rev() {
            out.extend(protocol.poll())
        }

        out
    }

    pub fn connected(&mut self, id: DeviceId, state: DeviceMode) {
        for protocol in self.protocols.iter_mut().rev() {
            protocol.connected(id, state);
        }
    }

    pub fn disconnected(&mut self, id: DeviceId) {
        for protocol in self.protocols.iter_mut().rev() {
            protocol.disconnected(id);
        }
    }

    #[must_use]
    pub fn clean_finished(&mut self) -> bool {
        let mut i = self.protocols.len();
        let mut send_cancel_to_all = false;
        while i > 0 {
            i -= 1;
            let protocol = &self.protocols[i];
            match protocol.is_complete() {
                Some(completion) => {
                    let name = protocol.name();
                    self.protocols.remove(i);
                    event!(
                        Level::INFO,
                        name = name,
                        outcome = format!("{:?}", completion),
                        stack_len = self.protocols.len(),
                        "UI Protocol completed",
                    );
                    match completion {
                        Completion::Success => { /* nothing to do */ }
                        Completion::Abort {
                            send_cancel_to_all_devices,
                        } => {
                            send_cancel_to_all |= send_cancel_to_all_devices;
                        }
                    }
                }
                None => { /* not complete */ }
            }
        }

        send_cancel_to_all
    }

    #[must_use]
    pub fn cancel_all(&mut self) -> bool {
        for protocol in self.protocols.iter_mut().rev() {
            protocol.cancel();
        }
        self.clean_finished()
    }

    pub fn push<T: UiProtocol>(&mut self, protocol: T) {
        let name = protocol.name();
        self.protocols.push(Box::new(protocol));
        event!(
            Level::INFO,
            stack_len = self.protocols.len(),
            name = name,
            "Added UI protocol to stack"
        );
    }
}

impl core::fmt::Debug for UiStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiStack")
            .field(
                "protocols",
                &self
                    .protocols
                    .iter()
                    .map(|proto| proto.name())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}
