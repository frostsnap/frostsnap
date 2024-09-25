use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::FrostCoordinator, message::CoordinatorToUserMessage, DeviceId, KeyId,
};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

pub struct DisplayBackupProtocol {
    device_id: DeviceId,
    complete: bool,
    abort: bool,
    messages: Vec<CoordinatorSendMessage>,
    // bool indicates whether it compelted successfully.
    // Only one message will be sent ATM
    sink: Box<dyn Sink<bool> + Send>,
}

impl DisplayBackupProtocol {
    pub fn new(
        coord: &mut FrostCoordinator,
        device_id: DeviceId,
        key_id: KeyId,
        sink: impl Sink<bool> + 'static,
    ) -> anyhow::Result<Self> {
        let messages = coord
            .request_device_display_backup(device_id, key_id)?
            .into_iter()
            .map(|message| {
                message
                    .try_into()
                    .expect("will only send messages to device")
            })
            .collect();

        Ok(Self {
            device_id,
            complete: false,
            sink: Box::new(sink),
            abort: false,
            messages,
        })
    }

    fn abort(&mut self) {
        self.abort = true;
        self.sink.send(false);
    }
}

impl UiProtocol for DisplayBackupProtocol {
    fn cancel(&mut self) {
        self.abort();
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.abort {
            Some(Completion::Abort {
                // get the devices to stop showing backup
                send_cancel_to_all_devices: true,
            })
        } else if self.complete {
            Some(Completion::Success)
        } else {
            None
        }
    }

    fn connected(&mut self, _id: frostsnap_core::DeviceId) {}

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.device_id == id {
            self.abort()
        }
    }

    fn process_to_user_message(
        &mut self,
        message: frostsnap_core::message::CoordinatorToUserMessage,
    ) {
        if let CoordinatorToUserMessage::DisplayBackupConfirmed { device_id } = message {
            if self.device_id == device_id {
                self.complete = true;
                self.sink.send(true);
            }
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        (core::mem::take(&mut self.messages), vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
