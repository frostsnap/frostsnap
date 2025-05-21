use crate::{Completion, DeviceMode, Sink, UiProtocol};
use frostsnap_comms::{CommsMisc, CoordinatorSendMessage};
use frostsnap_core::{coordinator::FrostCoordinator, AccessStructureRef, DeviceId, SymmetricKey};

pub struct DisplayBackupProtocol {
    device_id: DeviceId,
    abort: bool,
    messages: Vec<CoordinatorSendMessage>,
    should_send: bool,
    sink: Box<dyn Sink<bool> + Send>,
}

impl DisplayBackupProtocol {
    pub fn new(
        coord: &mut FrostCoordinator,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        sink: impl Sink<bool> + 'static,
    ) -> anyhow::Result<Self> {
        let messages = coord
            .request_device_display_backup(device_id, access_structure_ref, encryption_key)?
            .into_iter()
            .map(|message| {
                message
                    .try_into()
                    .expect("will only send messages to device")
            })
            .collect();

        Ok(Self {
            device_id,
            sink: Box::new(sink),
            abort: false,
            messages,
            should_send: true,
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
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn connected(&mut self, id: frostsnap_core::DeviceId, _state: DeviceMode) {
        if id == self.device_id {
            self.should_send = true;
        }
    }

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.device_id == id {
            self.abort()
        }
    }

    fn process_comms_message(&mut self, from: DeviceId, message: CommsMisc) -> bool {
        if self.device_id != from {
            return false;
        }
        if let CommsMisc::DisplayBackupConfrimed = message {
            self.sink.send(true);
            true
        } else {
            false
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        if !self.should_send {
            return vec![];
        }

        self.should_send = false;
        self.messages.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
