use alloc::{collections::VecDeque, string::ToString, vec::Vec};
use esp32c3_hal::{gpio, prelude::*, uart, UsbSerialJtag};

use crate::{
    io::{self, UpstreamDetector},
    state, storage,
    ui::{self, UiEvent, UserInteraction},
    ConnectionState,
};
use esp_storage::FlashStorage;
use frostsnap_comms::{CoordinatorSendBody, DeviceSendBody, DeviceSendMessage, ReceiveSerial};
use frostsnap_comms::{CoordinatorSendMessage, Downstream, MAGIC_BYTES_PERIOD};
use frostsnap_core::message::{
    CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage, DeviceToUserMessage,
};
use frostsnap_core::schnorr_fun::fun::marker::Normal;
use frostsnap_core::schnorr_fun::fun::KeyPair;
use frostsnap_core::schnorr_fun::fun::Scalar;
use frostsnap_core::DeviceId;

pub struct Run<'a, UpstreamUart, DownstreamUart, DownstreamDetect, Ui, T> {
    pub upstream_jtag: UsbSerialJtag<'a>,
    pub upstream_uart: uart::Uart<'a, UpstreamUart>,
    pub downstream_uart: uart::Uart<'a, DownstreamUart>,
    pub rng: esp32c3_hal::Rng<'a>,
    pub ui: Ui,
    pub timer: esp32c3_hal::timer::Timer<T>,
    pub downstream_detect: DownstreamDetect,
}

impl<'a, UpstreamUart, DownstreamUart, DownstreamDetect, Ui, T>
    Run<'a, UpstreamUart, DownstreamUart, DownstreamDetect, Ui, T>
where
    UpstreamUart: uart::Instance,
    DownstreamUart: uart::Instance,
    DownstreamDetect: gpio::InputPin,
    Ui: UserInteraction,
    T: esp32c3_hal::timer::Instance,
{
    pub fn run(self) -> ! {
        let Run {
            upstream_jtag,
            upstream_uart,
            downstream_uart,
            mut rng,
            mut ui,
            timer,
            downstream_detect,
        } = self;

        let flash = FlashStorage::new();
        let mut flash = storage::DeviceStorage::new(flash, storage::NVS_PARTITION_START);

        // Load state from Flash memory if available. If not, generate secret and save.
        let mut state = match flash.load() {
            Ok(state) => state,
            Err(_e) => {
                let mut rand_bytes = [0u8; 32];
                rng.read(&mut rand_bytes).unwrap();
                let secret = Scalar::from_bytes(rand_bytes).unwrap().non_zero().unwrap();
                let keypair: KeyPair = KeyPair::<Normal>::new(secret.clone());
                let frost_signer = frostsnap_core::FrostSigner::new(keypair);

                let state = state::FrostState {
                    signer: frost_signer.clone(),
                    name: None,
                };
                flash.save(&state).unwrap();
                state
            }
        };

        let device_id = state.signer.device_id();
        if let Some(name) = &state.name {
            ui.set_device_name(name.into());
        }

        let mut downstream_serial =
            io::SerialInterface::<_, _, Downstream>::new_uart(downstream_uart, &timer);
        let mut soft_reset = true;
        let mut downstream_connection_state = ConnectionState::Disconnected;
        let mut sends_downstream: Vec<CoordinatorSendMessage> = vec![];
        let mut sends_upstream = UpstreamSends::new(device_id);
        let mut sends_user: Vec<DeviceToUserMessage> = vec![];
        let mut outbox = VecDeque::new();
        let mut upstream_detector =
            UpstreamDetector::new(upstream_uart, upstream_jtag, &timer, MAGIC_BYTES_PERIOD);
        let mut upstream_sent_magic_bytes = false;
        let mut upstream_received_first_message = false;
        let mut next_write_magic_bytes = 0;
        // FIXME: If we keep getting magic bytes instead of getting a proper message we have to accept that
        // the upstream doesn't think we're awake yet and we should soft reset again and send our
        // magic bytes again.
        //
        // We wouldn't need this if announce ack was guaranteed to be sent right away (but instead
        // it waits until we've named it). Announcing and labeling has been sorted out this counter
        // thingy will go away naturally.
        let mut upstream_first_message_timeout_counter = 0;

        loop {
            if soft_reset {
                soft_reset = false;
                sends_upstream.messages.clear();
                let _ = state.signer.cancel_action();
                sends_user.clear();
                sends_downstream.clear();
                downstream_connection_state = ConnectionState::Disconnected;
                upstream_sent_magic_bytes = false;
                next_write_magic_bytes = 0;
                upstream_received_first_message = false;
                outbox.clear();
                sends_upstream.send(DeviceSendBody::Announce);
                sends_upstream.send(match &state.name {
                    Some(name) => DeviceSendBody::SetName { name: name.into() },
                    None => DeviceSendBody::NeedName,
                });
            }

            let is_usb_connected_downstream = !downstream_detect.is_input_high();

            match (is_usb_connected_downstream, downstream_connection_state) {
                (true, ConnectionState::Disconnected) => {
                    downstream_connection_state = ConnectionState::Connected;
                    ui.set_downstream_connection_state(downstream_connection_state);
                }
                (true, ConnectionState::Connected) => {
                    let now = timer.now();
                    if now > next_write_magic_bytes {
                        next_write_magic_bytes = now + 40_000 * MAGIC_BYTES_PERIOD;
                        downstream_serial
                            .write_magic_bytes()
                            .expect("couldn't write magic bytes downstream");
                    }
                    if downstream_serial.find_and_remove_magic_bytes() {
                        downstream_connection_state = ConnectionState::Established;
                        ui.set_downstream_connection_state(downstream_connection_state);
                        sends_upstream.send_debug("Device read magic bytes from another device!");
                    }
                }
                (
                    false,
                    state @ ConnectionState::Established | state @ ConnectionState::Connected,
                ) => {
                    downstream_connection_state = ConnectionState::Disconnected;
                    ui.set_downstream_connection_state(downstream_connection_state);
                    if state == ConnectionState::Established {
                        sends_upstream.send(DeviceSendBody::DisconnectDownstream);
                    }
                }
                _ => { /* nothing to do */ }
            }

            if downstream_connection_state == ConnectionState::Established {
                while let Some(device_send) = downstream_serial.receive() {
                    match device_send {
                        Ok(device_send) => {
                            match device_send {
                                ReceiveSerial::MagicBytes(_) => {
                                    sends_upstream.send_debug(
                                        "downstream device sent unexpected magic bytes",
                                    );
                                    // FIXME: decide what to do when downstream sends unexpected magic bytes
                                }
                                ReceiveSerial::Message(message) => {
                                    sends_upstream.messages.push(message)
                                }
                            };
                        }
                        Err(e) => {
                            sends_upstream
                                .send_debug(format!("Failed to decode on downstream port: {e}"));
                            downstream_connection_state = ConnectionState::Disconnected;
                        }
                    };
                }

                // Send messages downstream
                for send in sends_downstream.drain(..) {
                    downstream_serial.send(send).expect("sending downstream");
                }
            }

            match upstream_detector.serial_interface() {
                None => ui.set_workflow(ui::Workflow::WaitingFor(
                    ui::WaitingFor::LookingForUpstream {
                        jtag: upstream_detector.looking_at_jtag(),
                    },
                )),
                Some(upstream_serial) => {
                    if !upstream_sent_magic_bytes {
                        upstream_serial
                            .write_magic_bytes()
                            .expect("failed to write magic bytes");
                        upstream_sent_magic_bytes = true;
                        upstream_first_message_timeout_counter = 0;
                        ui.set_workflow(ui::Workflow::WaitingFor(
                            ui::WaitingFor::CoordinatorAnnounceAck,
                        ))
                    }

                    while let Some(received_message) = upstream_serial.receive() {
                        match received_message {
                            Ok(received_message) => {
                                match received_message {
                                    ReceiveSerial::MagicBytes(_) => {
                                        if upstream_received_first_message
                                            || upstream_first_message_timeout_counter > 10
                                        {
                                            soft_reset = true;
                                        } else {
                                            upstream_first_message_timeout_counter += 1;
                                        }
                                        continue;
                                    }
                                    ReceiveSerial::Message(mut message) => {
                                        // We have recieved a first message (if this is not a magic bytes message)
                                        upstream_received_first_message = true;
                                        let for_me = message
                                            .target_destinations
                                            .remove_from_recipients(device_id);

                                        if for_me {
                                            match &message.message_body {
                                                CoordinatorSendBody::Cancel => {
                                                    outbox.extend(state.signer.cancel_action());
                                                }
                                                CoordinatorSendBody::AnnounceAck => {
                                                    ui.set_workflow(ui::Workflow::WaitingFor(
                                                        ui::WaitingFor::CoordinatorInstruction {
                                                            completed_task: None,
                                                        },
                                                    ));
                                                    sends_upstream
                                                        .send_debug("Received AnnounceACK!");
                                                }
                                                CoordinatorSendBody::Naming(naming) => match naming
                                                {
                                                    frostsnap_comms::NameCommand::Preview(name) => {
                                                        ui.set_workflow(
                                                            ui::Workflow::NamingDevice {
                                                                old_name: state.name.clone(),
                                                                new_name: name.clone(),
                                                            },
                                                        );
                                                    }
                                                    frostsnap_comms::NameCommand::Finish(
                                                        new_name,
                                                    ) => {
                                                        ui.set_workflow(ui::Workflow::UserPrompt(
                                                            ui::Prompt::NewName {
                                                                old_name: state.name.clone(),
                                                                new_name: new_name.clone(),
                                                            },
                                                        ));
                                                    }
                                                },
                                                CoordinatorSendBody::Core(core_message) => {
                                                    // FIXME: It is very inelegant to be inspecting
                                                    // core messages to figure out when we're going
                                                    // to be busy.
                                                    match &core_message {
                                                        CoordinatorToDeviceMessage::DoKeyGen {
                                                            ..
                                                        } => {
                                                            ui.set_workflow(ui::Workflow::BusyDoing(
                                                                ui::BusyTask::KeyGen,
                                                            ));
                                                            state.signer.clear_state();
                                                        }
                                                        CoordinatorToDeviceMessage::FinishKeyGen {
                                                            ..
                                                        } => ui.set_workflow(ui::Workflow::BusyDoing(
                                                            ui::BusyTask::VerifyingShare,
                                                        )),
                                                        _ => { /* no workflow to trigger */ }
                                                    }

                                                    outbox.extend(
                                                        state
                                                            .signer
                                                            .recv_coordinator_message(
                                                                core_message.clone(),
                                                            )
                                                            .expect(
                                                                "failed to process coordinator message",
                                                            ),
                                                    );
                                                }
                                            }
                                        }

                                        // Forward messages downstream if there are other target destinations
                                        if downstream_connection_state
                                            == ConnectionState::Established
                                            && message.target_destinations.should_forward()
                                        {
                                            sends_downstream.push(message);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                panic!(
                                    "upstream read fail (got label: {}): {e}",
                                    ui.get_device_label().is_some(),
                                );
                            }
                        };
                    }

                    for send in sends_upstream.messages.drain(..) {
                        upstream_serial
                            .send(send)
                            .expect("unable to send to coordinator");
                    }
                }
            }

            if let Some(ui_event) = ui.poll() {
                match ui_event {
                    UiEvent::KeyGenConfirm => outbox.extend(
                        state
                            .signer
                            .keygen_ack()
                            .expect("state changed while confirming keygen"),
                    ),
                    UiEvent::SigningConfirm => {
                        ui.set_workflow(ui::Workflow::BusyDoing(ui::BusyTask::Signing));
                        outbox.extend(
                            state
                                .signer
                                .sign_ack()
                                .expect("state changed while acking sign"),
                        );
                    }
                    UiEvent::NameConfirm(ref name) => {
                        state.name = Some(name.into());
                        flash.save(&state).unwrap();
                        ui.set_device_name(name.into());
                        sends_upstream.send(DeviceSendBody::SetName { name: name.into() });
                    }
                }
                ui.set_workflow(ui::Workflow::WaitingFor(
                    ui::WaitingFor::CoordinatorInstruction {
                        completed_task: Some(ui_event.clone()),
                    },
                ));
            }

            // Handle message outbox to send: ToStorage, ToCoordinator, ToUser.
            // ⚠ pop_front ensures messages are sent in order. E.g. update nonce NVS before sending sig.
            while let Some(send) = outbox.pop_front() {
                match send {
                    DeviceSend::ToStorage(_) => {
                        flash.save(&state).unwrap();
                    }
                    DeviceSend::ToCoordinator(message) => {
                        if matches!(message, DeviceToCoordinatorMessage::KeyGenResponse(_)) {
                            ui.set_workflow(ui::Workflow::WaitingFor(
                                ui::WaitingFor::CoordinatorResponse(ui::WaitingResponse::KeyGen),
                            ));
                        }

                        sends_upstream.send(DeviceSendBody::Core(message));
                    }
                    DeviceSend::ToUser(user_send) => {
                        match user_send {
                            DeviceToUserMessage::CheckKeyGen { session_hash: xpub } => {
                                ui.set_workflow(ui::Workflow::UserPrompt(ui::Prompt::KeyGen(xpub)));
                            }
                            DeviceToUserMessage::SignatureRequest { sign_task, .. } => {
                                ui.set_workflow(ui::Workflow::UserPrompt(ui::Prompt::Signing(
                                    sign_task.to_string(),
                                )));
                            }
                            DeviceToUserMessage::Canceled { .. } => {
                                ui.cancel();
                            }
                        };
                    }
                }
            }
        }
    }
}

struct UpstreamSends {
    messages: Vec<DeviceSendMessage>,
    my_device_id: DeviceId,
}

impl UpstreamSends {
    fn new(my_device_id: DeviceId) -> Self {
        Self {
            messages: Default::default(),
            my_device_id,
        }
    }

    fn send(&mut self, body: DeviceSendBody) {
        self.messages.push(DeviceSendMessage {
            from: self.my_device_id,
            body,
        });
    }

    fn send_debug(&mut self, message: impl ToString) {
        self.send(DeviceSendBody::Debug {
            message: message.to_string(),
        })
    }
}
