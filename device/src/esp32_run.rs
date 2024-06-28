use crate::{
    io::SerialInterface,
    ota, storage,
    ui::{self, UiEvent, UserInteraction},
    DownstreamConnectionState, UpstreamConnection, UpstreamConnectionState,
};
use alloc::{collections::VecDeque, string::ToString, vec::Vec};
use esp_hal::{
    gpio,
    sha::Sha,
    timer::{self, timg::Timer},
    uart, Blocking,
};
use esp_storage::FlashStorage;
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceSendBody, DeviceSendMessage,
    ReceiveSerial, Upstream,
};
use frostsnap_comms::{CoordinatorSendMessage, Downstream, MAGIC_BYTES_PERIOD};
use frostsnap_core::{
    message::{
        CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage, DeviceToUserMessage,
    },
    schnorr_fun::fun::{marker::Normal, KeyPair, Scalar},
    DeviceId, FrostSigner,
};
use rand_chacha::rand_core::RngCore;

pub struct Run<'a, UpstreamUart, DownstreamUart, Rng, Ui, T, DownstreamDetectPin> {
    pub upstream_serial: SerialInterface<'a, T, UpstreamUart, Upstream>,
    pub downstream_serial: SerialInterface<'a, T, DownstreamUart, Downstream>,
    pub rng: Rng,
    pub ui: Ui,
    pub timer: &'a Timer<T, Blocking>,
    pub downstream_detect: gpio::Input<'a, DownstreamDetectPin>,
    pub sha256: Sha<'a, Blocking>,
}

impl<'a, UpstreamUart, DownstreamUart, Rng, Ui, T, DownstreamDetectPin>
    Run<'a, UpstreamUart, DownstreamUart, Rng, Ui, T, DownstreamDetectPin>
where
    UpstreamUart: uart::Instance,
    DownstreamUart: uart::Instance,
    DownstreamDetectPin: gpio::InputPin,
    Ui: UserInteraction,
    T: timer::timg::Instance,
    Rng: RngCore,
{
    pub fn run(self) -> ! {
        let Run {
            mut upstream_serial,
            mut downstream_serial,
            mut rng,
            mut ui,
            timer,
            downstream_detect,
            mut sha256,
        } = self;

        let flash = FlashStorage::new();
        let mut flash = storage::DeviceStorage::new(flash);
        let ota_config = ota::OtaConfig::new(flash.flash_mut());
        let active_partition = ota_config.active_partition(flash.flash_mut());
        let active_firmware_digest = active_partition.digest(flash.flash_mut(), &mut sha256);
        ui.set_busy_task(ui::BusyTask::Loading);

        let (mut signer, mut name) =
            match flash.read_header().expect("failed to read header from nvs") {
                Some(header) => {
                    let mut signer = FrostSigner::new(KeyPair::<Normal>::new(header.secret_key));
                    let mut name: Option<alloc::string::String> = None;

                    for change in flash.iter() {
                        match change {
                            storage::Change::Core(core) => {
                                signer.apply_change(core);
                            }
                            storage::Change::Name(name_update) => {
                                name = Some(name_update);
                            }
                        }
                    }
                    (signer, name)
                }
                None => {
                    let secret_key = Scalar::random(&mut rng);
                    let keypair = KeyPair::<Normal>::new(secret_key);
                    flash
                        .write_header(crate::storage::Header { secret_key })
                        .unwrap();
                    let signer = FrostSigner::new(keypair);
                    (signer, None)
                }
            };
        let device_id = signer.device_id();
        if let Some(name) = &name {
            ui.set_device_name(name.into());
        }

        let mut soft_reset = true;
        let mut downstream_connection_state = DownstreamConnectionState::Disconnected;
        let mut sends_downstream: Vec<CoordinatorSendMessage> = vec![];
        let mut sends_upstream = UpstreamSends::new(device_id);
        let mut sends_user: Vec<DeviceToUserMessage> = vec![];
        let mut outbox = VecDeque::new();
        let mut next_write_magic_bytes_downstream = 0;
        // FIXME: If we keep getting magic bytes instead of getting a proper message we have to accept that
        // the upstream doesn't think we're awake yet and we should soft reset again and send our
        // magic bytes again.
        //
        // We wouldn't need this if announce ack was guaranteed to be sent right away (but instead
        // it waits until we've named it). Announcing and labeling has been sorted out this counter
        // thingy will go away naturally.
        let mut upstream_first_message_timeout_counter = 0;

        ui.set_workflow(ui::Workflow::WaitingFor(
            ui::WaitingFor::LookingForUpstream {
                jtag: upstream_serial.is_jtag(),
            },
        ));

        let mut upstream_connection = UpstreamConnection {
            is_device: !upstream_serial.is_jtag(),
            state: UpstreamConnectionState::Connected,
        };

        ui.set_upstream_connection_state(upstream_connection);
        let mut upgrade: Option<ota::FirmwareUpgradeMode> = None;

        loop {
            if soft_reset {
                soft_reset = false;
                sends_upstream.messages.clear();
                let _ = signer.cancel_action();
                sends_user.clear();
                sends_downstream.clear();
                downstream_connection_state = DownstreamConnectionState::Disconnected;
                upstream_connection.state = UpstreamConnectionState::Connected;
                next_write_magic_bytes_downstream = 0;
                upgrade = None;
                outbox.clear();
                ui.cancel();
            }
            let is_usb_connected_downstream = !downstream_detect.is_high();

            // === DOWSTREAM connection management
            match (is_usb_connected_downstream, downstream_connection_state) {
                (true, DownstreamConnectionState::Disconnected) => {
                    downstream_connection_state = DownstreamConnectionState::Connected;
                    ui.set_downstream_connection_state(downstream_connection_state);
                }
                (true, DownstreamConnectionState::Connected) => {
                    let now = timer.now();
                    if now > next_write_magic_bytes_downstream {
                        next_write_magic_bytes_downstream = now + 80_000 * MAGIC_BYTES_PERIOD;
                        downstream_serial
                            .write_magic_bytes()
                            .expect("couldn't write magic bytes downstream");
                    }
                    if downstream_serial.find_and_remove_magic_bytes() {
                        downstream_connection_state = DownstreamConnectionState::Established;
                        ui.set_downstream_connection_state(downstream_connection_state);
                        sends_upstream.send_debug("Device read magic bytes from another device!");
                    }
                }
                (
                    false,
                    state @ DownstreamConnectionState::Established
                    | state @ DownstreamConnectionState::Connected,
                ) => {
                    downstream_connection_state = DownstreamConnectionState::Disconnected;
                    ui.set_downstream_connection_state(downstream_connection_state);
                    if state == DownstreamConnectionState::Established {
                        sends_upstream.push(DeviceSendBody::DisconnectDownstream);
                    }
                }
                _ => { /* nothing to do */ }
            }

            if downstream_connection_state == DownstreamConnectionState::Established {
                while let Some(device_send) = downstream_serial.receive() {
                    match device_send {
                        Ok(device_send) => {
                            match device_send {
                                ReceiveSerial::MagicBytes(_) => {
                                    sends_upstream.send_debug(
                                        "downstream device sent unexpected magic bytes",
                                    );
                                    // soft disconnect downstream device to reset it becasue it's
                                    // doing stuff we don't understand.
                                    sends_upstream.push(DeviceSendBody::DisconnectDownstream);
                                    downstream_connection_state =
                                        DownstreamConnectionState::Disconnected;
                                }
                                ReceiveSerial::Message(message) => {
                                    sends_upstream.messages.push(message)
                                }
                            };
                        }
                        Err(e) => {
                            sends_upstream
                                .send_debug(format!("Failed to decode on downstream port: {e}"));
                            sends_upstream.push(DeviceSendBody::DisconnectDownstream);
                            downstream_connection_state = DownstreamConnectionState::Disconnected;
                        }
                    };
                }

                // Send messages downstream
                for send in sends_downstream.drain(..) {
                    downstream_serial.send(send).expect("sending downstream");
                }
            }

            // === UPSTREAM connection management
            match upstream_connection.state {
                UpstreamConnectionState::Connected => {
                    if upstream_serial.find_and_remove_magic_bytes() {
                        upstream_serial
                            .write_magic_bytes()
                            .expect("failed to write magic bytes");
                        upstream_serial
                            .send(DeviceSendMessage {
                                from: device_id,
                                body: DeviceSendBody::Announce {
                                    firmware_digest: active_firmware_digest,
                                },
                            })
                            .expect("sending announce");
                        upstream_serial
                            .send(DeviceSendMessage {
                                from: device_id,
                                body: match &name {
                                    Some(name) => DeviceSendBody::SetName { name: name.into() },
                                    None => DeviceSendBody::NeedName,
                                },
                            })
                            .expect("sending name message");
                        upstream_connection.state = UpstreamConnectionState::Established;
                        ui.set_upstream_connection_state(upstream_connection);
                    }
                }
                upstream_state => {
                    while let Some(received_message) = upstream_serial.receive() {
                        match received_message {
                            Ok(received_message) => {
                                match received_message {
                                    ReceiveSerial::MagicBytes(_) => {
                                        if matches!(
                                            upstream_state,
                                            UpstreamConnectionState::EstablishedAndCoordAck
                                        ) {
                                            soft_reset = true;
                                        } else if upstream_first_message_timeout_counter > 1 {
                                            // We keep receving magic bytes so we reset the
                                            // connection and try announce again
                                            upstream_connection.state =
                                                UpstreamConnectionState::Connected;
                                            ui.set_upstream_connection_state(upstream_connection);
                                            upstream_first_message_timeout_counter = 0;
                                        } else {
                                            upstream_first_message_timeout_counter += 1;
                                        }
                                        continue;
                                    }
                                    ReceiveSerial::Message(mut message) => {
                                        // We have recieved a first message (if this is not a magic bytes message)
                                        let for_me = message
                                            .target_destinations
                                            .remove_from_recipients(device_id);

                                        if for_me {
                                            match &message.message_body {
                                                CoordinatorSendBody::Cancel => {
                                                    // FIXME: This is a mess -- can we redisign
                                                    // things so the "core" component doesn't need
                                                    // to know when it is canceled.
                                                    //
                                                    // We first ask the
                                                    // core logic to cancel what it's doing
                                                    let core_cancel = signer.cancel_action();
                                                    if core_cancel.is_none() {
                                                        // .. but if it's not doing anything then we
                                                        // are probably cancelling something in the
                                                        // ui
                                                        ui.cancel();
                                                    } else {
                                                        outbox.extend(core_cancel);
                                                    }
                                                }
                                                CoordinatorSendBody::AnnounceAck => {
                                                    upstream_connection.state = UpstreamConnectionState::EstablishedAndCoordAck;
                                                    ui.set_upstream_connection_state(
                                                        upstream_connection,
                                                    );
                                                }
                                                CoordinatorSendBody::Naming(naming) => match naming
                                                {
                                                    frostsnap_comms::NameCommand::Preview(
                                                        preview_name,
                                                    ) => {
                                                        ui.set_workflow(
                                                            ui::Workflow::NamingDevice {
                                                                old_name: name.clone(),
                                                                new_name: preview_name.clone(),
                                                            },
                                                        );
                                                    }
                                                    frostsnap_comms::NameCommand::Finish(
                                                        new_name,
                                                    ) => {
                                                        ui.set_workflow(ui::Workflow::UserPrompt(
                                                            ui::Prompt::NewName {
                                                                old_name: name.clone(),
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
                                                        } => ui.set_busy_task(ui::BusyTask::KeyGen),
                                                        CoordinatorToDeviceMessage::FinishKeyGen {
                                                            ..
                                                        } => ui.set_busy_task(ui::BusyTask::VerifyingShare),
                                                        _ => { /* no workflow to trigger */ }
                                                    }

                                                    outbox.extend(
                                                        signer
                                                            .recv_coordinator_message(
                                                                core_message.clone(),
                                                                &mut rng,
                                                            )
                                                            .expect(
                                                                "failed to process coordinator message",
                                                            ),
                                                    );
                                                }
                                                CoordinatorSendBody::Upgrade(upgrade_message) => {
                                                    match upgrade_message {
                                                        CoordinatorUpgradeMessage::PrepareUpgrade { size, firmware_digest } => {
                                                            let upgrade_ =
                                                                ota_config.start_upgrade(flash.flash_mut(), *size, *firmware_digest, active_firmware_digest);

                                                            upgrade = Some(upgrade_);
                                                        },
                                                        CoordinatorUpgradeMessage::EnterUpgradeMode => {
                                                            let downstream_io = if downstream_connection_state == DownstreamConnectionState::Established {
                                                                downstream_serial.send(message.clone()).expect("failed to init downstream upgrade");
                                                                Some(downstream_serial.inner_mut())
                                                            } else {
                                                                None
                                                            };

                                                            if let Some(upgrade) = &mut upgrade {
                                                                let upstream_io = upstream_serial.inner_mut();
                                                                upgrade.enter_upgrade_mode(flash.flash_mut(), upstream_io, downstream_io, &mut ui, &mut sha256);
                                                                esp_hal::reset::software_reset();
                                                            }
                                                            else {
                                                                panic!("upgrade cannot start because we were not warned about it")
                                                            }

                                                        },
                                                    }
                                                },
                                            }
                                        }

                                        // Forward messages downstream if there are other target destinations
                                        if downstream_connection_state
                                            == DownstreamConnectionState::Established
                                            && upstream_state
                                                == UpstreamConnectionState::EstablishedAndCoordAck
                                            && message.target_destinations.should_forward()
                                        {
                                            sends_downstream.push(message);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                panic!("upstream read fail:\n{}", e);
                            }
                        };
                    }

                    if let Some(upgrade_) = &mut upgrade {
                        let (message, workflow) = upgrade_.poll(flash.flash_mut());
                        sends_upstream.extend(message);
                        if let Some(workflow) = workflow {
                            ui.set_workflow(workflow);
                        }
                    }

                    if let UpstreamConnectionState::EstablishedAndCoordAck =
                        upstream_connection.state
                    {
                        for send in sends_upstream.messages.drain(..) {
                            upstream_serial
                                .send(send)
                                .expect("unable to send to coordinator");
                        }
                    }
                }
            }

            if let Some(ui_event) = ui.poll() {
                let mut switch_workflow = Some(ui::Workflow::WaitingFor(
                    ui::WaitingFor::CoordinatorInstruction {
                        completed_task: Some(ui_event.clone()),
                    },
                )); // this is just the default
                match ui_event {
                    UiEvent::KeyGenConfirm => outbox.extend(
                        signer
                            .keygen_ack()
                            .expect("state changed while confirming keygen"),
                    ),
                    UiEvent::SigningConfirm => {
                        ui.set_busy_task(ui::BusyTask::Signing);
                        outbox.extend(signer.sign_ack().expect("state changed while acking sign"));
                    }
                    UiEvent::NameConfirm(ref new_name) => {
                        name = Some(new_name.into());
                        flash
                            .push(storage::Change::Name(new_name.clone()))
                            .expect("flash write fail");
                        ui.set_device_name(new_name.into());
                        sends_upstream.push(DeviceSendBody::SetName {
                            name: new_name.into(),
                        });
                    }
                    UiEvent::BackupRequestConfirm(_) => {
                        outbox.extend(
                            signer
                                .display_backup_ack()
                                .expect("state changed while displaying backup"),
                        );
                    }
                    UiEvent::UpgradeConfirm { .. } => {
                        if let Some(upgrade) = upgrade.as_mut() {
                            upgrade.upgrade_confirm();
                        }
                        // special case where updrade will handle things from now on
                        switch_workflow = None;
                    }
                    UiEvent::EnteredShareBackup(backup_str) => outbox.extend(
                        signer
                            .restore_share(backup_str)
                            .expect("invalid state to restore share"),
                    ),
                }

                if let Some(switch_workflow) = switch_workflow {
                    ui.set_workflow(switch_workflow);
                }
            }
            // Handle message outbox to send: ToStorage, ToCoordinator, ToUser.
            // ⚠ pop_front ensures messages are sent in order. E.g. update nonce NVS before sending sig.
            while let Some(send) = outbox.pop_front() {
                match send {
                    DeviceSend::ToStorage(message) => {
                        flash.push(storage::Change::Core(message)).unwrap();
                    }
                    DeviceSend::ToCoordinator(message) => {
                        if matches!(message, DeviceToCoordinatorMessage::KeyGenResponse(_)) {
                            ui.set_workflow(ui::Workflow::WaitingFor(
                                ui::WaitingFor::CoordinatorResponse(ui::WaitingResponse::KeyGen),
                            ));
                        }

                        sends_upstream.push(DeviceSendBody::Core(message));
                    }
                    DeviceSend::ToUser(user_send) => {
                        match user_send {
                            DeviceToUserMessage::CheckKeyGen { session_hash } => {
                                ui.set_workflow(ui::Workflow::UserPrompt(ui::Prompt::KeyGen(
                                    session_hash,
                                )));
                            }
                            DeviceToUserMessage::SignatureRequest { sign_task, .. } => {
                                ui.set_workflow(ui::Workflow::UserPrompt(ui::Prompt::Signing(
                                    sign_task.to_string(),
                                )));
                            }
                            DeviceToUserMessage::DisplayBackupRequest { key_id } => ui
                                .set_workflow(ui::Workflow::UserPrompt(
                                    ui::Prompt::DisplayBackupRequest(key_id),
                                )),
                            DeviceToUserMessage::Canceled { .. } => {
                                ui.cancel();
                            }
                            DeviceToUserMessage::DisplayBackup { key_id: _, backup } => {
                                ui.set_workflow(ui::Workflow::DisplayBackup { backup });
                            }
                            DeviceToUserMessage::RestoreBackup => {
                                ui.set_workflow(ui::Workflow::RestoringShare);
                            }
                        };
                    }
                }
            }
        }
    }
}

/// simple mechanism to attach our device id to outgoing messages
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

    fn push(&mut self, body: DeviceSendBody) {
        self.messages.push(DeviceSendMessage {
            from: self.my_device_id,
            body,
        });
    }

    fn extend(&mut self, iter: impl IntoIterator<Item = DeviceSendBody>) {
        self.messages
            .extend(iter.into_iter().map(|body| DeviceSendMessage {
                from: self.my_device_id,
                body,
            }));
    }

    fn send_debug(&mut self, message: impl ToString) {
        self.push(DeviceSendBody::Debug {
            message: message.to_string(),
        })
    }
}
