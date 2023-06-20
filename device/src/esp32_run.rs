use alloc::{collections::VecDeque, string::ToString, vec::Vec};
use esp32c3_hal::{clock::Clocks, peripherals::USB_DEVICE, prelude::*, uart, Delay, UsbSerialJtag};

use crate::{
    io::{self, UpstreamDetector},
    state, storage,
};
use esp_storage::FlashStorage;
use frostsnap_comms::{
    DeviceReceiveBody, DeviceReceiveSerial, DeviceSendMessage, DeviceSendSerial,
};
use frostsnap_comms::{DeviceReceiveMessage, Downstream};
use frostsnap_core::message::{
    CoordinatorToDeviceMessage, DeviceSend, DeviceToUserMessage, SignTask,
};
use frostsnap_core::schnorr_fun::fun::hex;
use frostsnap_core::schnorr_fun::fun::marker::Normal;
use frostsnap_core::schnorr_fun::fun::KeyPair;
use frostsnap_core::schnorr_fun::fun::Scalar;

pub trait UserInteraction {
    fn splash_screen(&mut self);

    fn waiting_for_upstream(&mut self, looking_at_jtag: bool);

    fn await_instructions(&mut self, name: &str);

    fn confirm_sign(&mut self, sign_task: &SignTask);

    // TODO: This needs to check a transcript of the session
    fn confirm_key_generated(&mut self, xpub: &str);

    fn display_error(&mut self, message: &str);

    fn poll(&mut self) -> Option<UiEvent>;

    /// try not to use this
    fn misc_print(&mut self, string: &str);
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    KeyGenConfirm(bool),
    SigningConfirm(bool),
}

pub struct Run<'a, UpstreamUart, DownstreamUart, Ui, T> {
    pub upstream_jtag: UsbSerialJtag<'a, USB_DEVICE>,
    pub upstream_uart: uart::Uart<'a, UpstreamUart>,
    pub downstream_uart: uart::Uart<'a, DownstreamUart>,
    pub clocks: Clocks<'a>,
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
            clocks,
            mut rng,
            mut ui,
            timer,
        } = self;
        let mut delay = Delay::new(&clocks);

        let flash = FlashStorage::new();
        let mut flash = storage::DeviceStorage::new(flash, storage::NVS_PARTITION_START);

        // Welcome screen
        // Some delay before turning on backlight to hide screen flicker
        ui.splash_screen();

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

        delay.delay_ms(1_000u32);

        let mut downstream_serial =
            io::SerialInterface::<_, _, Downstream>::new_uart(downstream_uart, &timer);
        let mut soft_reset = true;
        let mut downstream_active = false;
        let mut sends_downstream: Vec<DeviceReceiveMessage> = vec![];
        let mut sends_upstream: Vec<DeviceSendMessage> = vec![];
        let mut sends_user: Vec<DeviceToUserMessage> = vec![];
        let mut outbox = VecDeque::new();
        let mut upstream_detector = UpstreamDetector::new(upstream_uart, upstream_jtag, &timer);
        let mut upstream_sent_magic_bytes = false;
        let mut upstream_received_first_message = false;
        let mut next_write_magic_bytes = 0;

        loop {
            if soft_reset {
                ui.misc_print("soft resetting");
                delay.delay_ms(500u32);
                soft_reset = false;
                sends_upstream = vec![DeviceSendMessage::Announce(frostsnap_comms::Announce {
                    from: frost_signer.device_id(),
                })];
                sends_user.clear();
                sends_downstream.clear();
                downstream_active = false;
                upstream_sent_magic_bytes = false;
                next_write_magic_bytes = 0;
                upstream_received_first_message = false;
                outbox.clear();
            }

            if downstream_active {
                if downstream_serial.poll_read() {
                    match downstream_serial.receive_from_downstream() {
                        Ok(device_send) => {
                            let forward_upstream = match device_send {
                                DeviceSendSerial::MagicBytes(_) => {
                                    // soft reset downstream if it sends unexpected magic bytes so we restablish
                                    // downstream_active = false;
                                    DeviceSendMessage::Debug {
                                        message: format!(
                                            "downstream device sent unexpected magic bytes"
                                        ),
                                        device: frost_signer.device_id(),
                                    }
                                }
                                DeviceSendSerial::Message(message) => match message {
                                    DeviceSendMessage::Core(core) => DeviceSendMessage::Core(core),
                                    DeviceSendMessage::Debug { message, device } => {
                                        DeviceSendMessage::Debug { message, device }
                                    }
                                    DeviceSendMessage::Announce(message) => {
                                        DeviceSendMessage::Announce(message)
                                    }
                                },
                            };
                            sends_upstream.push(forward_upstream);
                        }
                        Err(e) => {
                            sends_upstream.push(DeviceSendMessage::Debug {
                                message: format!("Failed to decode on downstream port: {e}"),
                                device: frost_signer.device_id(),
                            });
                            downstream_active = false;
                        }
                    };
                }

                // Send messages downstream
                for send in sends_downstream.drain(..) {
                    downstream_serial
                        .forward_downstream(DeviceReceiveSerial::Message(send))
                        .expect("sending downstream");
                }
            } else {
                let now = timer.now();
                if now > next_write_magic_bytes {
                    next_write_magic_bytes = now + 40_000 * 100;
                    downstream_serial
                        .write_magic_bytes()
                        .expect("couldn't write magic bytes downstream");
                }
                if downstream_serial.find_and_remove_magic_bytes() {
                    downstream_active = true;
                    sends_upstream.push(DeviceSendMessage::Debug {
                        message: "Device read magic bytes from another device!".to_string(),
                        device: frost_signer.clone().device_id(),
                    });
                }
            }

            if upstream_detector.serial_interface().is_none() {
                ui.waiting_for_upstream(upstream_detector.looking_at_jtag());
            }

            if let Some(upstream_serial) = upstream_detector.serial_interface() {
                if !upstream_sent_magic_bytes {
                    upstream_serial
                        .write_magic_bytes()
                        .expect("failed to write magic bytes");
                    upstream_sent_magic_bytes = true;
                }

                if upstream_serial.poll_read() {
                    let prior_to_read_buff = upstream_serial.read_buffer().to_vec();

                    match upstream_serial.receive_from_coordinator() {
                        Ok(received_message) => {
                            match received_message {
                                DeviceReceiveSerial::MagicBytes(_) => {
                                    if upstream_received_first_message {
                                        soft_reset = true;
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
                                        if forwarding_message.target_destinations.len() > 0 {
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
                                        DeviceReceiveBody::AnnounceAck { device_label, .. } => {
                                            ui.await_instructions(&device_label);
                                            sends_upstream.push(DeviceSendMessage::Debug {
                                                message: "Received AnnounceACK!".to_string(),
                                                device: frost_signer.device_id(),
                                            });
                                        }
                                        DeviceReceiveBody::Core(core_message) => {
                                            if let CoordinatorToDeviceMessage::DoKeyGen {
                                                devices,
                                                ..
                                            } = &core_message
                                            {
                                                if devices.contains(&frost_signer.device_id()) {
                                                    frost_signer.clear_state();
                                                }
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
                        Err(_) => {
                            sends_upstream.push(DeviceSendMessage::Debug {
                                message: format!(
                                    "Device failed to read upstream: {}",
                                    hex::encode(&prior_to_read_buff)
                                ),
                                device: frost_signer.device_id(),
                            });
                            panic!("upstream read fail: {}", hex::encode(&prior_to_read_buff));
                        }
                    };
                }

                for send in sends_upstream.drain(..) {
                    upstream_serial
                        .send_to_coodinator(DeviceSendSerial::Message(send.clone()))
                        .expect("unable to send to coordinator");
                }
            }

            if let Some(ui_event) = ui.poll() {
                let outgoing = match ui_event {
                    UiEvent::KeyGenConfirm(ack) => {
                        frost_signer.keygen_ack(ack).expect("We must still be waiting for keygen ack")
                    },
                    UiEvent::SigningConfirm(ack) => {
                        frost_signer.sign_ack(ack).expect("We must still be waiting for signing ack")
                    }
                };

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
                        sends_upstream.push(DeviceSendMessage::Core(message));
                    }
                    DeviceSend::ToUser(user_send) => {
                        match user_send {
                            DeviceToUserMessage::CheckKeyGen { xpub } => {
                                ui.confirm_key_generated(&xpub);
                            }
                            frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                                message_to_sign,
                                ..
                            } => {
                                ui.confirm_sign(&message_to_sign);
                            }
                        };
                    }
                }
            }
        }
    }
}
