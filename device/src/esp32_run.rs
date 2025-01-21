use core::cell::RefCell;

use crate::{
    efuse::EfuseHmacKeys,
    flash_nonce_slot,
    io::SerialInterface,
    ota, storage,
    ui::{self, UiEvent, UserInteraction},
    DownstreamConnectionState, Duration, Instant, UpstreamConnection, UpstreamConnectionState,
};
use alloc::{boxed::Box, collections::VecDeque, rc::Rc, string::ToString, vec::Vec};
use esp_hal::{gpio, sha::Sha, timer};
use esp_storage::FlashStorage;
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceSendBody, ReceiveSerial, Upstream,
};
use frostsnap_comms::{Downstream, MAGIC_BYTES_PERIOD};
use frostsnap_core::{
    device::FrostSigner,
    message::{
        CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage, DeviceToUserMessage,
    },
    schnorr_fun::fun::{marker::Normal, KeyPair, Scalar},
    SignTask,
};
use rand_chacha::rand_core::RngCore;

pub struct Run<'a, Rng, Ui, T, DownstreamDetectPin> {
    pub upstream_serial: SerialInterface<'a, T, Upstream>,
    pub downstream_serial: SerialInterface<'a, T, Downstream>,
    pub rng: Rng,
    pub ui: Ui,
    pub timer: &'a T,
    pub downstream_detect: gpio::Input<'a, DownstreamDetectPin>,
    pub sha256: Sha<'a>,
    pub hmac_keys: EfuseHmacKeys<'a>,
}

impl<Rng, Ui, T, DownstreamDetectPin> Run<'_, Rng, Ui, T, DownstreamDetectPin>
where
    DownstreamDetectPin: gpio::InputPin,
    Ui: UserInteraction,
    T: timer::Timer,
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
            mut hmac_keys,
        } = self;

        let flash = Rc::new(RefCell::new(FlashStorage::new()));
        let flash_nonce_slots = flash_nonce_slot::flash_nonce_slots(flash.clone());
        let ota_config = ota::OtaFlash::new(flash.clone());
        let mut app_flash = storage::DeviceStorage::new(flash.clone());
        let active_partition = ota_config.active_partition();
        let active_firmware_digest = active_partition.digest(&mut sha256);
        ui.set_busy_task(ui::BusyTask::Loading);

        let (mut signer, mut name) = match app_flash
            .read_header()
            .expect("failed to read header from nvs")
        {
            Some(header) => {
                let mut signer =
                    FrostSigner::new(KeyPair::<Normal>::new(header.secret_key), flash_nonce_slots);
                let mut name: Option<alloc::string::String> = None;

                for change in app_flash.iter() {
                    match change {
                        storage::Change::Core(mutation) => {
                            signer.apply_mutation(&mutation);
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
                app_flash
                    .write_header(crate::storage::Header { secret_key })
                    .unwrap();
                let signer = FrostSigner::new(keypair, flash_nonce_slots);
                (signer, None)
            }
        };
        let device_id = signer.device_id();
        if let Some(name) = &name {
            ui.set_device_name(name.into());
        }

        let mut soft_reset = true;
        let mut downstream_connection_state = DownstreamConnectionState::Disconnected;
        let mut sends_user: Vec<DeviceToUserMessage> = vec![];
        let mut outbox = VecDeque::new();
        let mut inbox: Vec<CoordinatorSendBody> = vec![];
        let mut next_write_magic_bytes_downstream: Instant = Instant::from_ticks(0);
        // If we keep getting magic bytes instead of getting a proper message we have to accept that
        // the upstream doesn't think we're awake yet and we should soft reset.
        let mut magic_bytes_timeout_counter = 0;

        ui.set_workflow(ui::Workflow::WaitingFor(
            ui::WaitingFor::LookingForUpstream {
                jtag: upstream_serial.is_jtag(),
            },
        ));

        let mut upstream_connection = UpstreamConnection::new(device_id);

        ui.set_upstream_connection_state(upstream_connection.state);
        // HACK: During alpha testing we say direct to coordinator is always ready. Later on we can
        // just have coordinator signal ready -- but that would break old devices atm.
        let mut upgrade: Option<ota::FirmwareUpgradeMode> = None;
        let mut ui_event_queue = VecDeque::default();
        let mut conch_is_downstream = false;

        loop {
            let mut has_conch = false;
            if soft_reset {
                soft_reset = false;
                conch_is_downstream = false;
                magic_bytes_timeout_counter = 0;
                let _ = signer.cancel_action();
                sends_user.clear();
                downstream_connection_state = DownstreamConnectionState::Disconnected;
                upstream_connection.set_state(UpstreamConnectionState::PowerOn, &mut ui);
                next_write_magic_bytes_downstream = Instant::from_ticks(0);
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
                        next_write_magic_bytes_downstream = now
                            .checked_add_duration(Duration::millis(MAGIC_BYTES_PERIOD))
                            .expect("won't overflow");
                        downstream_serial
                            .write_magic_bytes()
                            .expect("couldn't write magic bytes downstream");
                    }
                    if downstream_serial.find_and_remove_magic_bytes() {
                        downstream_connection_state = DownstreamConnectionState::Established;
                        ui.set_downstream_connection_state(downstream_connection_state);
                        upstream_connection
                            .send_debug("Device read magic bytes from another device!");
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
                        upstream_connection
                            .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
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
                                    upstream_connection.send_debug(
                                        "downstream device sent unexpected magic bytes",
                                    );
                                    // soft disconnect downstream device to reset it becasue it's
                                    // doing stuff we don't understand.
                                    upstream_connection.send_to_coordinator([
                                        DeviceSendBody::DisconnectDownstream,
                                    ]);
                                    downstream_connection_state =
                                        DownstreamConnectionState::Disconnected;
                                }
                                ReceiveSerial::Message(message) => {
                                    conch_is_downstream = false;
                                    upstream_serial
                                        .send(message)
                                        .expect("failed to forward message");
                                }
                                ReceiveSerial::Conch => {
                                    conch_is_downstream = false;
                                    upstream_serial
                                        .write_conch()
                                        .expect("failed to write conch upstream");
                                }
                                _ => { /* unused */ }
                            };
                        }
                        Err(e) => {
                            upstream_connection
                                .send_debug(format!("Failed to decode on downstream port: {e}"));
                            upstream_connection
                                .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
                            downstream_connection_state = DownstreamConnectionState::Disconnected;
                            break;
                        }
                    };
                }
            }

            if downstream_connection_state != DownstreamConnectionState::Established
                && conch_is_downstream
            {
                // We take the conch back since downstream disconnected
                has_conch = true;
                conch_is_downstream = false;
            }

            // === UPSTREAM connection management
            match upstream_connection.get_state() {
                UpstreamConnectionState::PowerOn => {
                    if upstream_serial.find_and_remove_magic_bytes() {
                        upstream_serial
                            .write_magic_bytes()
                            .expect("failed to write magic bytes");
                        upstream_connection.send_announcement(DeviceSendBody::Announce {
                            firmware_digest: active_firmware_digest,
                        });
                        upstream_connection.send_to_coordinator([match &name {
                            Some(name) => DeviceSendBody::SetName { name: name.into() },
                            None => DeviceSendBody::NeedName,
                        }]);

                        upstream_connection
                            .set_state(UpstreamConnectionState::Established, &mut ui);
                    }
                }
                _ => {
                    let mut last_message_was_magic_bytes = false;
                    while let Some(received_message) = upstream_serial.receive() {
                        match received_message {
                            Ok(received_message) => {
                                last_message_was_magic_bytes =
                                    matches!(received_message, ReceiveSerial::MagicBytes(_));
                                match received_message {
                                    ReceiveSerial::Message(mut message) => {
                                        // We have recieved a first message (if this is not a magic bytes message)
                                        let for_me = message
                                            .target_destinations
                                            .remove_from_recipients(device_id);

                                        // Forward messages downstream if there are other target destinations
                                        if downstream_connection_state
                                            == DownstreamConnectionState::Established
                                            && message.target_destinations.should_forward()
                                        {
                                            downstream_serial
                                                .send(message.clone())
                                                .expect("sending downstream");
                                        }

                                        if for_me {
                                            match message.message_body.decode() {
                                                // Upgrade mode must be handled eagerly
                                                Some(CoordinatorSendBody::Upgrade(
                                                    CoordinatorUpgradeMessage::EnterUpgradeMode,
                                                )) => {
                                                    if let Some(upgrade) = &mut upgrade {
                                                        let upstream_io =
                                                            upstream_serial.inner_mut();
                                                        upgrade.enter_upgrade_mode(
                                                            upstream_io,
                                                            if downstream_connection_state == DownstreamConnectionState::Established { Some(downstream_serial.inner_mut()) } else { None },
                                                            &mut ui,
                                                            &mut sha256,
                                                        );
                                                        esp_hal::reset::software_reset();
                                                    } else {
                                                        panic!("upgrade cannot start because we were not warned about it")
                                                    }
                                                }
                                                Some(decoded) => {
                                                    inbox.push(decoded);
                                                }
                                                _ => { /* unable to decode so ignore */ }
                                            }
                                        }
                                    }
                                    ReceiveSerial::Conch => {
                                        assert!(
                                            !conch_is_downstream,
                                            "conch shouldn't be downstream if we receive it"
                                        );
                                        assert!(
                                            !has_conch,
                                            "we shouldn't have the conch if coordinator sends it"
                                        );

                                        has_conch = true;
                                        conch_is_downstream = false;
                                    }
                                    _ => { /* unused */ }
                                }
                            }
                            Err(e) => {
                                panic!("upstream read fail:\n{}", e);
                            }
                        };
                    }

                    let is_upstream_established = matches!(
                        upstream_connection.get_state(),
                        UpstreamConnectionState::EstablishedAndCoordAck
                    );

                    if last_message_was_magic_bytes {
                        if is_upstream_established {
                            // We get unexpected magic bytes after receiving normal messages.
                            // Upstream must have reset so we should reset.
                            soft_reset = true;
                        } else if magic_bytes_timeout_counter > 1 {
                            // We keep receving magic bytes so we reset the
                            // connection and try announce again.
                            upstream_connection
                                .set_state(UpstreamConnectionState::PowerOn, &mut ui);
                            magic_bytes_timeout_counter = 0;
                        } else {
                            magic_bytes_timeout_counter += 1;
                        }
                    }

                    if let Some(upgrade_) = &mut upgrade {
                        let (message, workflow) = upgrade_.poll();
                        upstream_connection.send_to_coordinator(message);

                        if let Some(workflow) = workflow {
                            ui.set_workflow(workflow);
                        }
                    }
                }
            }

            if let Some(ui_event) = ui.poll() {
                ui_event_queue.push_back(ui_event);
            }

            if !has_conch && conch_is_downstream {
                // we don't have the conch so we shouldn't do any more work -- just restart the loop
                continue;
            }

            for message_body in inbox.drain(..) {
                match &message_body {
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
                        upgrade = None;
                    }
                    CoordinatorSendBody::AnnounceAck => {
                        upstream_connection
                            .set_state(UpstreamConnectionState::EstablishedAndCoordAck, &mut ui);
                    }
                    CoordinatorSendBody::Naming(naming) => match naming {
                        frostsnap_comms::NameCommand::Preview(preview_name) => {
                            ui.set_workflow(ui::Workflow::NamingDevice {
                                old_name: name.clone(),
                                new_name: preview_name.clone(),
                            });
                        }
                        frostsnap_comms::NameCommand::Finish(new_name) => {
                            ui.set_workflow(ui::Workflow::prompt(ui::Prompt::NewName {
                                old_name: name.clone(),
                                new_name: new_name.clone(),
                            }));
                        }
                    },
                    CoordinatorSendBody::Core(core_message) => {
                        // FIXME: It is very inelegant to be inspecting
                        // core messages to figure out when we're going
                        // to be busy.
                        let mut is_busy = true;
                        match &core_message {
                            CoordinatorToDeviceMessage::DoKeyGen { .. } => {
                                ui.set_busy_task(ui::BusyTask::KeyGen)
                            }
                            CoordinatorToDeviceMessage::FinishKeyGen { .. } => {
                                ui.set_busy_task(ui::BusyTask::VerifyingShare)
                            }
                            CoordinatorToDeviceMessage::OpenNonceStreams { .. } => {
                                ui.set_busy_task(ui::BusyTask::GeneratingNonces);
                            }
                            _ => is_busy = false,
                        }

                        outbox.extend(
                            signer
                                .recv_coordinator_message(core_message.clone(), &mut rng)
                                .expect("failed to process coordinator message"),
                        );

                        if is_busy {
                            ui.clear_workflow();
                        }
                    }
                    CoordinatorSendBody::Upgrade(upgrade_message) => match upgrade_message {
                        CoordinatorUpgradeMessage::PrepareUpgrade {
                            size,
                            firmware_digest,
                        } => {
                            let upgrade_ = ota_config.start_upgrade(
                                *size,
                                *firmware_digest,
                                active_firmware_digest,
                            );

                            upgrade = Some(upgrade_);
                        }
                        CoordinatorUpgradeMessage::EnterUpgradeMode => {}
                    },
                    CoordinatorSendBody::DataWipe => {
                        ui.set_workflow(ui::Workflow::prompt(ui::Prompt::WipeDevice))
                    }
                }
            }

            {
                let staged_mutations = signer.staged_mutations();
                if !staged_mutations.is_empty() {
                    let now = self.timer.now();
                    // ⚠ Apply any mutations made to flash before outputting anything to user or to coordinator
                    app_flash
                        .append(staged_mutations.drain(..).map(storage::Change::Core))
                        .expect("writing core mutations failed");
                    let after = self.timer.now().checked_duration_since(now).unwrap();
                    upstream_connection
                        .send_debug(format!("core mutations took {}ms", after.to_millis()));
                }
            }

            // Handle message outbox to send: ToCoordinator, ToUser.
            // ⚠ pop_front ensures messages are sent in order.
            while let Some(send) = outbox.pop_front() {
                match send {
                    DeviceSend::ToCoordinator(boxed) => {
                        if matches!(
                            boxed.as_ref(),
                            DeviceToCoordinatorMessage::KeyGenResponse(_)
                        ) {
                            ui.set_workflow(ui::Workflow::WaitingFor(
                                ui::WaitingFor::CoordinatorResponse(ui::WaitingResponse::KeyGen),
                            ));
                        }

                        upstream_connection.send_to_coordinator([DeviceSendBody::Core(*boxed)]);
                    }
                    DeviceSend::ToUser(boxed) => {
                        match *boxed {
                            DeviceToUserMessage::CheckKeyGen {
                                key_id: _,
                                session_hash,
                                key_name,
                                t_of_n,
                            } => {
                                ui.set_workflow(ui::Workflow::prompt(ui::Prompt::KeyGen {
                                    session_hash,
                                    key_name,
                                    t_of_n,
                                }));
                            }
                            DeviceToUserMessage::VerifyAddress {
                                address,
                                bip32_path,
                            } => {
                                let rand_seed = rng.next_u32();
                                ui.set_workflow(ui::Workflow::DisplayAddress {
                                    address: address.to_string(),
                                    bip32_path: bip32_path
                                        .path_segments_from_bitcoin_appkey()
                                        .map(|i| i.to_string())
                                        .collect::<Vec<_>>()
                                        .join("/"),
                                    rand_seed,
                                })
                            }
                            DeviceToUserMessage::SignatureRequest { sign_task } => {
                                ui.set_workflow(ui::Workflow::prompt(ui::Prompt::Signing(
                                    match sign_task.sign_task {
                                        SignTask::Plain { message } => {
                                            ui::SignPrompt::Plain(message)
                                        }
                                        SignTask::Nostr { event } => {
                                            ui::SignPrompt::Nostr(event.content)
                                        }
                                        SignTask::BitcoinTransaction(transaction) => {
                                            let network = signer
                                                .wallet_network(sign_task.master_appkey.key_id())
                                                .expect("asked to sign a bitcoin transaction that doesn't support bitcoin");

                                            let fee = transaction.fee().expect("transaction validity should have already been checked");
                                            let foreign_recipients = transaction
                                                .foreign_recipients()
                                                .map(|(spk, value)| {
                                                    (
                                                        bitcoin::Address::from_script(spk, network)
                                                            .expect("has address representation"),
                                                        bitcoin::Amount::from_sat(value),
                                                    )
                                                })
                                                .collect::<Vec<_>>();
                                            ui::SignPrompt::Bitcoin {
                                                foreign_recipients,
                                                fee: bitcoin::Amount::from_sat(fee),
                                            }
                                        }
                                    },
                                )));
                            }
                            DeviceToUserMessage::DisplayBackupRequest { key_name, key_id } => ui
                                .set_workflow(ui::Workflow::prompt(
                                    ui::Prompt::DisplayBackupRequest { key_name, key_id },
                                )),
                            DeviceToUserMessage::Canceled { .. } => {
                                ui.cancel();
                            }
                            DeviceToUserMessage::DisplayBackup { key_name, backup } => {
                                ui.set_workflow(ui::Workflow::DisplayBackup { key_name, backup });
                            }
                            DeviceToUserMessage::EnterBackup => {
                                ui.set_workflow(ui::Workflow::EnteringBackup(
                                    ui::EnteringBackupStage::Init,
                                ));
                            }
                            DeviceToUserMessage::EnteredBackup(share_backup) => {
                                ui.set_workflow(ui::Workflow::prompt(
                                    ui::Prompt::ConfirmLoadBackup(share_backup),
                                ));
                            }
                        };
                    }
                }
            }

            for ui_event in ui_event_queue.drain(..) {
                let mut switch_workflow = Some(ui::Workflow::WaitingFor(
                    ui::WaitingFor::CoordinatorInstruction {
                        completed_task: Some(ui_event.clone()),
                    },
                )); // this is just the default

                match ui_event {
                    UiEvent::KeyGenConfirm => {
                        outbox.extend(
                            signer
                                .keygen_ack(&mut hmac_keys.share_encryption, &mut rng)
                                .expect("state changed while confirming keygen"),
                        );
                    }
                    UiEvent::SigningConfirm => {
                        ui.set_busy_task(ui::BusyTask::Signing);
                        outbox.extend(
                            signer
                                .sign_ack(&mut hmac_keys.share_encryption)
                                .expect("state changed while acking sign"),
                        );
                    }
                    UiEvent::NameConfirm(ref new_name) => {
                        name = Some(new_name.into());
                        app_flash
                            .append([storage::Change::Name(new_name.clone())])
                            .expect("flash write fail");
                        ui.set_device_name(new_name.into());
                        upstream_connection.send_to_coordinator([DeviceSendBody::SetName {
                            name: new_name.into(),
                        }]);
                    }
                    UiEvent::BackupRequestConfirm => {
                        outbox.extend(
                            signer
                                .display_backup_ack(&mut hmac_keys.share_encryption)
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
                    UiEvent::EnteredShareBackup(share_backup) => {
                        outbox.push_back(DeviceSend::ToUser(Box::new(
                            DeviceToUserMessage::EnteredBackup(share_backup),
                        )))
                    }
                    UiEvent::EnteredShareBackupConfirm(share_backup) => {
                        outbox.extend(
                            signer
                                .loaded_share_backup(share_backup)
                                .expect("invalid state to restore share"),
                        );
                    }
                    UiEvent::WipeDataConfirm => {
                        app_flash.erase().expect("erasing flash storage failed");
                        esp_hal::reset::software_reset();
                    }
                }

                if let Some(switch_workflow) = switch_workflow {
                    ui.set_workflow(switch_workflow);
                }
            }

            if has_conch {
                if let Some(message) = upstream_connection.dequeue_message() {
                    upstream_serial
                        .send(message.into())
                        .expect("failed to send message upstream");
                } else if downstream_connection_state == DownstreamConnectionState::Established {
                    conch_is_downstream = true;
                    downstream_serial.write_conch().unwrap();
                } else {
                    conch_is_downstream = false;
                    upstream_serial.write_conch().unwrap();
                }
            }
        }
    }
}
