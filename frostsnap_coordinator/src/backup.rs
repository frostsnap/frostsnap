use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{message::CoordinatorToUserMessage, DeviceId};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

pub struct BackupProtocol {
    device_id: DeviceId,
    complete: bool,
    sink: Box<dyn Sink<()> + Send>,
}

impl BackupProtocol {
    pub fn new(device_id: DeviceId, sink: impl Sink<()> + 'static) -> Self {
        Self {
            device_id,
            complete: false,
            sink: Box::new(sink),
        }
    }
}

impl UiProtocol for BackupProtocol {
    fn cancel(&mut self) {
        self.complete = true;
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.complete {
            Some(Completion::Abort {
                // get the devices to stop showing backup
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn connected(&mut self, _id: frostsnap_core::DeviceId) {}

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.device_id == id {
            self.complete = true;
        }
    }

    fn process_to_user_message(
        &mut self,
        message: frostsnap_core::message::CoordinatorToUserMessage,
    ) {
        if let CoordinatorToUserMessage::DisplayBackupConfirmed { device_id } = message {
            if self.device_id == device_id {
                self.sink.send(());
            }
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        (vec![], vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
