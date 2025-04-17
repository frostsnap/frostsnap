use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    coordinator::{
        restoration::{self, RecoverShare},
        CoordinatorToUserMessage,
    },
    message::{CoordinatorRestoration, CoordinatorToDeviceMessage},
    DeviceId,
};
use std::collections::BTreeSet;

use crate::{DeviceMode, Sink, UiProtocol};

pub struct WaitForRecoveryShare {
    sent_request_to: BTreeSet<DeviceId>,
    state: WaitForRecoveryShareState,
    sink: Box<dyn Sink<WaitForRecoveryShareState>>,
    abort: bool,
}

impl WaitForRecoveryShare {
    pub fn new(sink: impl Sink<WaitForRecoveryShareState>) -> Self {
        Self {
            sent_request_to: Default::default(),
            state: Default::default(),
            sink: Box::new(sink),
            abort: false,
        }
    }
}

impl WaitForRecoveryShare {
    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }
}

impl UiProtocol for WaitForRecoveryShare {
    fn cancel(&mut self) {
        self.abort = true;
    }

    fn is_complete(&self) -> Option<crate::Completion> {
        if self.abort {
            Some(crate::Completion::Abort {
                send_cancel_to_all_devices: false,
            })
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        self.state.connected.remove(&id);
        self.state.blank.remove(&id);
        self.sent_request_to.remove(&id);
        for list in [&mut self.state.recoverable, &mut self.state.already_got] {
            list.retain(|candidate| candidate.held_by != id);
        }

        self.emit_state();
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        let mut out = vec![];
        let need_to_send_to = self
            .state
            .connected
            .difference(&self.sent_request_to)
            .cloned()
            .collect::<Vec<_>>();

        for device_id in need_to_send_to {
            out.push(CoordinatorSendMessage {
                target_destinations: Destination::Particular([device_id].into()),
                message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(
                    CoordinatorRestoration::RequestHeldShares,
                )),
            });
            self.sent_request_to.insert(device_id);
        }
        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn connected(&mut self, id: DeviceId, state: DeviceMode) {
        match state {
            DeviceMode::Blank => self.state.blank.insert(id),
            _ => self.state.connected.insert(id),
        };
        self.emit_state();
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) {
        if let CoordinatorToUserMessage::Restoration(
            restoration::ToUserRestoration::GotHeldShares {
                held_by,
                already_got,
                recoverable,
            },
        ) = message
        {
            self.state
                .recoverable
                .extend(recoverable.into_iter().map(|held_share| RecoverShare {
                    held_by,
                    held_share,
                }));
            self.state
                .already_got
                .extend(already_got.into_iter().map(|held_share| RecoverShare {
                    held_by,
                    held_share,
                }));
            self.emit_state();
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WaitForRecoveryShareState {
    pub recoverable: Vec<RecoverShare>,
    pub already_got: Vec<RecoverShare>,
    pub connected: BTreeSet<DeviceId>,
    pub blank: BTreeSet<DeviceId>,
}
