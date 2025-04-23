use std::{collections::BTreeSet, sync};

use frostsnap_core::{coordinator::CoordinatorToUserMessage, DeviceId};

use crate::UiProtocol;

pub struct WaitForToUserMessage<F> {
    callback: F,
    cancel_on_disconnected: BTreeSet<DeviceId>,
    finished: Option<bool>,
    sender: Option<sync::mpsc::SyncSender<bool>>,
}

pub type Waiter = sync::mpsc::Receiver<bool>;

impl<F> WaitForToUserMessage<F> {
    pub fn new(devices: impl IntoIterator<Item = DeviceId>, callback: F) -> (Self, Waiter) {
        let (sender, recv) = sync::mpsc::sync_channel(1);
        (
            Self {
                callback,
                cancel_on_disconnected: devices.into_iter().collect(),
                finished: None,
                sender: Some(sender),
            },
            recv,
        )
    }
}

impl<F> UiProtocol for WaitForToUserMessage<F>
where
    F: FnMut(CoordinatorToUserMessage) -> bool + Send + 'static,
{
    fn is_complete(&self) -> Option<crate::Completion> {
        match self.finished {
            Some(false) => Some(crate::Completion::Abort {
                send_cancel_to_all_devices: false,
            }),
            Some(true) => Some(crate::Completion::Success),
            None => None,
        }
    }

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.cancel_on_disconnected.contains(&id) {
            self.finished.get_or_insert(false);
            if let Some(sender) = self.sender.take() {
                sender.send(false).unwrap();
            }
        }
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) {
        if (self.callback)(message) {
            self.finished.get_or_insert(true);
            if let Some(sender) = self.sender.take() {
                sender.send(true).unwrap();
            }
        }
    }

    fn poll(&mut self) -> Vec<frostsnap_comms::CoordinatorSendMessage> {
        vec![]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
