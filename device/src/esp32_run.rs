use alloc::{collections::VecDeque, string::ToString, vec::Vec};
use esp32c3_hal::{prelude::*, uart, UsbSerialJtag};

use crate::{
    io::{self, UpstreamDetector},
    state, storage,
    ui::{self, UiEvent, UserInteraction},
};
use esp_storage::FlashStorage;
use frostsnap_comms::{
    DeviceReceiveBody, DeviceReceiveSerial, DeviceSendMessage, DeviceSendMessageBody,
    DeviceSendSerial,
};
use frostsnap_comms::{DeviceReceiveMessage, Downstream, MAGIC_BYTES_PERIOD};
use frostsnap_core::message::{
    CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage, DeviceToUserMessage,
};
use frostsnap_core::schnorr_fun::fun::marker::Normal;
use frostsnap_core::schnorr_fun::fun::KeyPair;
use frostsnap_core::schnorr_fun::fun::Scalar;

pub struct Run<'a, UpstreamUart, DownstreamUart, Ui, T> {
    pub upstream_jtag: UsbSerialJtag<'a>,
    pub upstream_uart: uart::Uart<'a, UpstreamUart>,
    pub downstream_uart: uart::Uart<'a, DownstreamUart>,
    pub rng: esp32c3_hal::Rng<'a>,
    pub ui: Ui,
    pub timer: esp32c3_hal::timer::Timer<T>,
}

impl<'a, UpstreamUart, DownstreamUart, Ui, T> Run<'a, UpstreamUart, DownstreamUart, Ui, T>
where
    UpstreamUart: uart::Instance,
    DownstreamUart: uart::Instance,
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
        } = self;

        let flash = FlashStorage::new();
        let mut flash = storage::DeviceStorage::new(flash, storage::NVS_PARTITION_START);

        // Load state from Flash memory if available. If not, generate secret and save.
        let mut frost_signer = match flash.load() {
            Ok(state) => state.signer,
            Err(_e) => {
                let mut rand_bytes = [0u8; 32];
                rng.read(&mut rand_bytes).unwrap();
                let secret = Scalar::from_bytes(rand_bytes).unwrap().non_zero().unwrap();
                let keypair: KeyPair = KeyPair::<Normal>::new(secret.clone());
                let frost_signer = frostsnap_core::FrostSigner::new(keypair);

                flash
                    .save(&state::FrostState {
                        signer: frost_signer.clone(),
                    })
                    .unwrap();
                ui.misc_print("New secret generated and saved");
                frost_signer
            }
        };

        let mut downstream_serial =
            io::SerialInterface::<_, _, Downstream>::new_uart(downstream_uart, &timer);
        let mut soft_reset = true;
        let mut downstream_active = false;
        let mut sends_downstream: Vec<DeviceReceiveMessage> = vec![];
        let mut sends_upstream: Vec<DeviceSendMessageBody> = vec![];
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
                sends_upstream = vec![DeviceSendMessageBody::Announce(
                    frostsnap_comms::Announce {},
                )];
                frost_signer.cancel_action();
                sends_user.clear();
                sends_downstream.clear();
                downstream_active = false;
                upstream_sent_magic_bytes = false;
                next_write_magic_bytes = 0;
                upstream_received_first_message = false;
                outbox.clear();
            }

            if downstream_active {
                while let Some(device_send) = downstream_serial.receive_from_downstream() {
                    match device_send {
                        Ok(device_send) => {
                            let forward_upstream = match device_send {
                                DeviceSendSerial::MagicBytes(_) => {
                                    // FIXME: decide what to do when downstream sends unexpected magic bytes
                                    DeviceSendMessageBody::Debug {
                                        message: "downstream device sent unexpected magic bytes"
                                            .to_string(),
                                    }
                                }
                                DeviceSendSerial::Message(message) => match message.body {
                                    DeviceSendMessageBody::Core(core) => {
                                        DeviceSendMessageBody::Core(core)
                                    }
                                    DeviceSendMessageBody::Debug { message } => {
                                        DeviceSendMessageBody::Debug { message }
                                    }
                                    DeviceSendMessageBody::Announce(message) => {
                                        DeviceSendMessageBody::Announce(message)
                                    }
                                },
                            };
                            sends_upstream.push(forward_upstream);
                        }
                        Err(e) => {
                            sends_upstream.push(DeviceSendMessageBody::Debug {
                                message: format!("Failed to decode on downstream port: {e}"),
                            });
                            downstream_active = false;
                            ui.set_downstream_connection_state(false);
                        }
                    };
                }

                // Send messages downstream
                for send in sends_downstream.drain(..) {
                    downstream_serial
                        .forward_downstream(DeviceReceiveSerial::Message(send.clone()))
                        .expect("sending downstream");
                }
            } else {
                let now = timer.now();
                if now > next_write_magic_bytes {
                    next_write_magic_bytes = now + 40_000 * MAGIC_BYTES_PERIOD;
                    downstream_serial
                        .write_magic_bytes()
                        .expect("couldn't write magic bytes downstream");
                }
                if downstream_serial.find_and_remove_magic_bytes() {
                    downstream_active = true;
                    ui.set_downstream_connection_state(true);
                    sends_upstream.push(DeviceSendMessageBody::Debug {
                        message: "Device read magic bytes from another device!".to_string(),
                    });
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

                    while let Some(received_message) = upstream_serial.receive_from_coordinator() {
                        match received_message {
                            Ok(received_message) => {
                                match received_message {
                                    DeviceReceiveSerial::MagicBytes(_) => {
                                        if upstream_received_first_message
                                            || upstream_first_message_timeout_counter > 10
                                        {
                                            soft_reset = true;
                                        } else {
                                            upstream_first_message_timeout_counter += 1;
                                        }
                                        continue;
                                    }
                                    DeviceReceiveSerial::Message(message) => {
                                        // We have recieved a first message (if this is not a magic bytes message)
                                        upstream_received_first_message = true;
                                        // Forward messages downstream if there are other target destinations
                                        if downstream_active {
                                            let mut forwarding_message = message.clone();
                                            let _ = forwarding_message
                                                .target_destinations
                                                .remove(&frost_signer.device_id());
                                            if !forwarding_message.target_destinations.is_empty() {
                                                sends_downstream.push(forwarding_message);
                                            }
                                        }
                                        // Skip processing of messages which are not destined for us
                                        if !message
                                            .target_destinations
                                            .contains(&frost_signer.device_id())
                                        {
                                            continue;
                                        }

                                        match message.message_body {
                                            DeviceReceiveBody::AnnounceAck { device_label } => {
                                                ui.set_device_label(device_label);
                                                ui.set_workflow(ui::Workflow::WaitingFor(
                                                    ui::WaitingFor::CoordinatorInstruction {
                                                        completed_task: None,
                                                    },
                                                ));
                                                sends_upstream.push(DeviceSendMessageBody::Debug {
                                                    message: "Received AnnounceACK!".to_string(),
                                                });
                                            }
                                            DeviceReceiveBody::Core(core_message) => {
                                                match &core_message {
                                                    CoordinatorToDeviceMessage::DoKeyGen {
                                                        ..
                                                    } => {
                                                        ui.set_workflow(ui::Workflow::BusyDoing(
                                                            ui::BusyTask::KeyGen,
                                                        ));
                                                        frost_signer.clear_state();
                                                    }
                                                    CoordinatorToDeviceMessage::FinishKeyGen {
                                                        ..
                                                    } => ui.set_workflow(ui::Workflow::BusyDoing(
                                                        ui::BusyTask::VerifyingShare,
                                                    )),
                                                    CoordinatorToDeviceMessage::RequestSign {
                                                        ..
                                                    } => { /* no workflow to trigger */ }
                                                }

                                                match frost_signer
                                                    .recv_coordinator_message(core_message.clone())
                                                {
                                                    Ok(new_sends) => {
                                                        outbox.extend(new_sends);
                                                    }
                                                    Err(e) => {
                                                        ui.display_error(&e.gist());
                                                    }
                                                }
                                            }
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

                    for send in sends_upstream.drain(..) {
                        upstream_serial
                            .send_to_coodinator(DeviceSendSerial::Message(DeviceSendMessage {
                                body: send.clone(),
                                from: frost_signer.device_id(),
                            }))
                            .expect("unable to send to coordinator");
                    }
                }
            }

            if let Some(ui_event) = ui.poll() {
                let outgoing = match ui_event {
                    UiEvent::KeyGenConfirm(ack) => frost_signer.keygen_ack(ack),
                    UiEvent::SigningConfirm(ack) => {
                        if ack {
                            ui.set_workflow(ui::Workflow::BusyDoing(ui::BusyTask::Signing));
                        }
                        frost_signer.sign_ack(ack)
                    }
                }
                .expect("core state should not change without changing workflow");

                ui.set_workflow(ui::Workflow::WaitingFor(
                    ui::WaitingFor::CoordinatorInstruction {
                        completed_task: Some(ui_event.clone()),
                    },
                ));

                outbox.extend(outgoing)
            }

            // Handle message outbox to send: ToStorage, ToCoordinator, ToUser.
            // ⚠ pop_front ensures messages are sent in order. E.g. update nonce NVS before sending sig.
            while let Some(send) = outbox.pop_front() {
                match send {
                    DeviceSend::ToStorage(_) => {
                        flash
                            .save(&state::FrostState {
                                signer: frost_signer.clone(),
                            })
                            .unwrap();
                    }
                    DeviceSend::ToCoordinator(message) => {
                        if matches!(message, DeviceToCoordinatorMessage::KeyGenResponse(_)) {
                            ui.set_workflow(ui::Workflow::WaitingFor(
                                ui::WaitingFor::CoordinatorResponse(ui::WaitingResponse::KeyGen),
                            ));
                        }

                        sends_upstream.push(DeviceSendMessageBody::Core(message));
                    }
                    DeviceSend::ToUser(user_send) => {
                        match user_send {
                            DeviceToUserMessage::CheckKeyGen { xpub } => {
                                ui.set_workflow(ui::Workflow::UserPrompt(ui::Prompt::KeyGen(xpub)));
                            }
                            frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                                sign_task,
                                ..
                            } => {
                                ui.set_workflow(ui::Workflow::UserPrompt(ui::Prompt::Signing(
                                    sign_task.to_string(),
                                )));
                            }
                        };
                    }
                }
            }
        }
    }
}
