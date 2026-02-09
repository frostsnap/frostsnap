use frostsnap_comms::{CommsMisc, CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_core::DeviceId;
use std::borrow::BorrowMut;

use crate::{Completion, Sink, UiProtocol};

#[derive(Clone, Debug, PartialEq)]
pub enum EraseDeviceState {
    /// Waiting for user to confirm erase on device
    WaitingForConfirmation,
    /// Device confirmed erase has started (first sector erased)
    Confirmed,
}

pub struct EraseDevice {
    target_device: DeviceId,
    sent_request: bool,
    confirmed: bool,
    aborted: bool,
    sink: Box<dyn Sink<EraseDeviceState>>,
}

impl EraseDevice {
    pub fn new(target_device: DeviceId, sink: impl Sink<EraseDeviceState> + 'static) -> Self {
        Self {
            target_device,
            sent_request: false,
            confirmed: false,
            aborted: false,
            sink: Box::new(sink),
        }
    }
}

impl UiProtocol for EraseDevice {
    fn cancel(&mut self) {
        self.aborted = true;
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.aborted {
            Some(Completion::Abort {
                send_cancel_to_all_devices: false,
            })
        } else if self.confirmed {
            Some(Completion::Success)
        } else {
            None
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        if !self.sent_request {
            self.sent_request = true;
            self.sink.send(EraseDeviceState::WaitingForConfirmation);
            vec![CoordinatorSendMessage::to(
                self.target_device,
                CoordinatorSendBody::DataErase,
            )]
        } else {
            vec![]
        }
    }

    fn process_comms_message(&mut self, from: DeviceId, message: CommsMisc) -> bool {
        if from == self.target_device && matches!(message, CommsMisc::EraseConfirmed) {
            self.confirmed = true;
            self.sink.send(EraseDeviceState::Confirmed);
            true
        } else {
            false
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self.borrow_mut()
    }
}
