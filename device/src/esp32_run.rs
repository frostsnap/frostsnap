//! Main event loop for the device

use crate::partitions::PartitionExt;
use crate::{
    ds,
    flash::{Mutation, MutationLog},
    io::SerialInterface,
    ota,
    resources::Resources,
    ui::{self, UiEvent, UserInteraction},
    DownstreamConnectionState, Duration, Instant, UpstreamConnection, UpstreamConnectionState,
};
use alloc::{boxed::Box, collections::VecDeque, string::ToString, vec::Vec};
use esp_hal::timer::Timer;
use frostsnap_comms::{
    CommsMisc, CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceSendBody, ReceiveSerial,
    Upstream, MAGIC_BYTES_PERIOD,
};
use frostsnap_core::{
    device::{DeviceToUserMessage, FrostSigner},
    message::{DeviceSend, DeviceToCoordinatorMessage},
};
use frostsnap_embedded::NonceAbSlot;
use rand_core::RngCore;

/// Main event loop for the device
pub fn run<'a>(resources: &'a mut Resources<'a>) -> ! {
    // Destructure resources
    let Resources {
        ref mut rng,
        ref mut hmac_keys,
        ds: ref mut hardware_rsa,
        ref certificate,
        ref mut nvs,
        ota: ref mut ota_partitions,
        ref mut ui,
        ref mut timer,
        ref mut sha256,
        ref mut uart_upstream,
        ref mut uart_downstream,
        ref mut jtag,
        ref mut upstream_detect,
        ref mut downstream_detect,
        ref mut rsa,
    } = resources;

    // Read device header and keypair from NVS
    let header_sectors = nvs.split_off_front(2);
    let header_flash = crate::flash::FlashHeader::new(header_sectors);
    let header = match header_flash.read_header() {
        Some(h) => h,
        None => {
            // New device - verify NVS is empty
            if !nvs.is_empty().expect("checking NVS is empty") {
                panic!("Device appears to be new but NVS is not blank. Maybe you need to manually erase the device?");
            }
            // Initialize new header with device keypair
            header_flash.init(rng)
        }
    };
    let device_keypair = header.device_keypair(&mut hmac_keys.fixed_entropy);

    // Set up NVS partitions for shares, nonces, and mutation log
    let share_partition = nvs.split_off_front(2);
    let nonce_slots = {
        // Give half the remaining nvs over to nonces
        let mut n_nonce_sectors = nvs.n_sectors().div_ceil(2);
        // Make sure it's a multiple of 2
        n_nonce_sectors = (n_nonce_sectors.div_ceil(2) * 2).max(16);
        // Each nonce slot requires 2 sectors so divide by 2 to get the number of slots
        NonceAbSlot::load_slots(nvs.split_off_front(n_nonce_sectors))
    };

    // The event log gets the rest of the sectors
    let mut mutation_log = MutationLog::new(share_partition, *nvs);

    // Initialize signer with device keypair and nonce slots
    let mut signer = FrostSigner::new(device_keypair, nonce_slots);

    // Apply any existing mutations from the log
    let mut name = None;
    for change in mutation_log.seek_iter() {
        match change {
            Ok(change) => match change {
                Mutation::Core(mutation) => {
                    signer.apply_mutation(mutation);
                }
                Mutation::Name(name_update) => {
                    name = Some(name_update);
                }
            },
            Err(e) => {
                panic!("failed to read event: {e}");
            }
        }
    }

    // Set recovery mode if we have saved backups
    if !signer.saved_backups().is_empty() {
        ui.set_recovery_mode(true);
    }

    // Get active firmware information
    let active_partition = ota_partitions.active_partition();
    let firmware_size = active_partition.firmware_size().unwrap();
    let active_firmware_digest = active_partition.sha256_digest(sha256, Some(firmware_size));

    // Set device name if we have one
    let device_id = signer.device_id();
    if let Some(name) = &name {
        ui.set_device_name(name.into());
    }

    // Create serial interfaces
    let mut upstream_serial: SerialInterface<_, Upstream> = if upstream_detect.is_low() {
        SerialInterface::new_uart(uart_upstream.as_mut().unwrap(), timer)
    } else {
        SerialInterface::new_jtag(jtag, timer)
    };
    let mut downstream_serial: SerialInterface<_, frostsnap_comms::Downstream> =
        SerialInterface::new_uart(uart_downstream, timer);

    // Initialize state variables
    let mut soft_reset = true;
    let mut downstream_connection_state = DownstreamConnectionState::Disconnected;
    let mut sends_user: Vec<DeviceToUserMessage> = vec![];
    let mut outbox = VecDeque::new();
    let mut inbox: Vec<CoordinatorSendBody> = vec![];
    let mut next_write_magic_bytes_downstream: Instant = Instant::from_ticks(0);
    let mut magic_bytes_timeout_counter = 0;

    ui.set_workflow(ui::Workflow::WaitingFor(
        ui::WaitingFor::LookingForUpstream {
            jtag: upstream_serial.is_jtag(),
        },
    ));

    let mut upstream_connection = UpstreamConnection::new(device_id);
    ui.set_upstream_connection_state(upstream_connection.state);
    let mut upgrade: Option<ota::FirmwareUpgradeMode> = None;
    let mut ui_event_queue = VecDeque::default();
    let mut conch_is_downstream = false;

    ui.clear_busy_task();

    // Main event loop
    loop {
        let mut has_conch = false;

        if soft_reset {
            soft_reset = false;
            conch_is_downstream = false;
            magic_bytes_timeout_counter = 0;
            signer.clear_tmp_data();
            sends_user.clear();
            downstream_connection_state = DownstreamConnectionState::Disconnected;
            upstream_connection.set_state(UpstreamConnectionState::PowerOn, ui);
            next_write_magic_bytes_downstream = Instant::from_ticks(0);
            upgrade = None;
            ui.set_device_name(name.clone());
            outbox.clear();
            ui.cancel();
        }

        let is_usb_connected_downstream = !downstream_detect.is_high();

        // === DOWNSTREAM connection management
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
                    upstream_connection.send_debug("Device read magic bytes from another device!");
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
                    upstream_connection.send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
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
                                upstream_connection
                                    .send_debug("downstream device sent unexpected magic bytes");
                                // Soft disconnect downstream device to reset it
                                upstream_connection
                                    .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
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
                            ReceiveSerial::Reset => {
                                upstream_connection
                                    .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
                                downstream_connection_state =
                                    DownstreamConnectionState::Disconnected;
                                break;
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

                    upstream_connection.set_state(UpstreamConnectionState::Established, ui);
                }
            }
            _ => {
                let mut last_message_was_magic_bytes = false;
                while let Some(received_message) = upstream_serial.receive() {
                    match received_message {
                        Ok(received_message) => {
                            let received_message: ReceiveSerial<Upstream> = received_message;
                            last_message_was_magic_bytes =
                                matches!(received_message, ReceiveSerial::MagicBytes(_));
                            match received_message {
                                ReceiveSerial::Message(mut message) => {
                                    let for_me: bool = message
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
                                                    let upstream_io = upstream_serial.inner_mut();
                                                    upgrade.enter_upgrade_mode(
                                                        upstream_io,
                                                        if downstream_connection_state == DownstreamConnectionState::Established {
                                                            Some(downstream_serial.inner_mut())
                                                        } else {
                                                            None
                                                        },
                                                        ui,
                                                        sha256,
                                                        timer,
                                                        rsa,
                                                    );
                                                    reset(&mut upstream_serial);
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
                                ReceiveSerial::Reset => { /* upstream doesn't send this */ }
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
                        // We keep receiving magic bytes so we reset the
                        // connection and try announce again.
                        upstream_connection.set_state(UpstreamConnectionState::PowerOn, ui);
                        magic_bytes_timeout_counter = 0;
                    } else {
                        magic_bytes_timeout_counter += 1;
                    }
                }

                if let Some(upgrade_) = &mut upgrade {
                    let message = upgrade_.poll(ui);
                    upstream_connection.send_to_coordinator(message);
                }
            }
        }

        if let Some(ui_event) = ui.poll() {
            ui_event_queue.push_back(ui_event);
        }

        if has_conch || !conch_is_downstream {
            // Process inbox messages
            for message_body in inbox.drain(..) {
                match &message_body {
                    CoordinatorSendBody::Cancel => {
                        signer.clear_tmp_data();
                        ui.cancel();
                        ui.set_device_name(name.clone());
                        upgrade = None;
                    }
                    CoordinatorSendBody::AnnounceAck => {
                        upstream_connection
                            .set_state(UpstreamConnectionState::EstablishedAndCoordAck, ui);
                    }
                    CoordinatorSendBody::Naming(naming) => match naming {
                        frostsnap_comms::NameCommand::Preview(preview_name) => {
                            ui.set_device_name(Some(preview_name));
                        }
                        frostsnap_comms::NameCommand::Prompt(new_name) => {
                            ui.set_workflow(ui::Workflow::prompt(ui::Prompt::NewName {
                                old_name: name.clone(),
                                new_name: new_name.clone(),
                            }));
                        }
                    },
                    CoordinatorSendBody::Core(core_message) => {
                        outbox.extend(
                            signer
                                .recv_coordinator_message(
                                    core_message.clone(),
                                    rng,
                                    &mut hmac_keys.share_encryption,
                                )
                                .expect("failed to process coordinator message"),
                        );
                        ui.clear_busy_task();
                    }
                    CoordinatorSendBody::Upgrade(upgrade_message) => match upgrade_message {
                        CoordinatorUpgradeMessage::PrepareUpgrade {
                            size,
                            firmware_digest,
                        } => {
                            let upgrade_ = ota_partitions.start_upgrade(
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
                    CoordinatorSendBody::Challenge(challenge) => {
                        // Can only respond if we have hardware RSA and certificate
                        if let (Some(hw_rsa), Some(cert)) =
                            (hardware_rsa.as_mut(), certificate.as_ref())
                        {
                            let signature = hw_rsa.sign(challenge.as_ref(), sha256);

                            let signature = ds::words_to_bytes(&signature);
                            upstream_connection.send_to_coordinator([
                                DeviceSendBody::SignedChallenge {
                                    signature: Box::new(signature),
                                    certificate: Box::new(cert.clone()),
                                },
                            ]);
                        }
                    }
                }
            }

            // Apply any staged mutations
            {
                let staged_mutations = signer.staged_mutations();
                if !staged_mutations.is_empty() {
                    let now = timer.now();
                    mutation_log
                        .append(staged_mutations.drain(..).map(Mutation::Core))
                        .expect("writing core mutations failed");
                    let after = timer.now().checked_duration_since(now).unwrap();
                    upstream_connection
                        .send_debug(format!("core mutations took {}ms", after.to_millis()));
                }
            }

            // Handle message outbox to send
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
                            DeviceToUserMessage::FinalizeKeyGen => {
                                let new_name = ui
                                    .get_device_name()
                                    .expect("must have set name before starting keygen")
                                    .to_string();
                                name = Some(new_name.clone());
                                mutation_log
                                    .push(Mutation::Name(new_name.clone()))
                                    .expect("flash write fail");
                                upstream_connection.send_to_coordinator([
                                    DeviceSendBody::SetName { name: new_name },
                                ]);
                                ui.clear_busy_task();
                                ui.clear_workflow();
                            }
                            DeviceToUserMessage::CheckKeyGen { phase, .. } => {
                                ui.set_workflow(ui::Workflow::prompt(ui::Prompt::KeyGen { phase }));
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
                            DeviceToUserMessage::SignatureRequest { phase } => {
                                ui.set_workflow(ui::Workflow::prompt(ui::Prompt::Signing {
                                    phase,
                                }));
                            }
                            DeviceToUserMessage::Restoration(to_user_restoration) => {
                                use frostsnap_core::device::restoration::ToUserRestoration::*;
                                match to_user_restoration {
                                    DisplayBackupRequest { phase } => {
                                        ui.set_workflow(ui::Workflow::prompt(
                                            ui::Prompt::DisplayBackupRequest { phase },
                                        ));
                                    }
                                    DisplayBackup { key_name, backup } => {
                                        ui.set_workflow(ui::Workflow::DisplayBackup {
                                            key_name,
                                            backup,
                                        });
                                    }
                                    EnterBackup { phase } => {
                                        ui.set_workflow(ui::Workflow::EnteringBackup(
                                            ui::EnteringBackupStage::Init { phase },
                                        ));
                                    }
                                    BackupSaved { .. } => {
                                        ui.set_recovery_mode(true);
                                    }
                                    ConsolidateBackup(phase) => {
                                        // Automatically confirm consolidation
                                        outbox.extend(signer.finish_consolidation(
                                            &mut hmac_keys.share_encryption,
                                            phase,
                                            rng,
                                        ));
                                        ui.set_recovery_mode(false);
                                    }
                                }
                            }
                        };
                    }
                }
            }

            // Handle UI events
            for ui_event in ui_event_queue.drain(..) {
                let mut switch_workflow = Some(ui::Workflow::WaitingFor(
                    ui::WaitingFor::CoordinatorInstruction {
                        completed_task: Some(ui_event.clone()),
                    },
                ));

                match ui_event {
                    UiEvent::KeyGenConfirm { phase } => {
                        let waiting_for = ui::WaitingFor::WaitingForKeyGenFinalize {
                            key_name: phase.key_name().to_string(),
                            t_of_n: phase.t_of_n(),
                            session_hash: phase.session_hash(),
                        };
                        outbox.extend(
                            signer
                                .keygen_ack(*phase, &mut hmac_keys.share_encryption, rng)
                                .expect("state changed while confirming keygen"),
                        );
                        switch_workflow = Some(ui::Workflow::WaitingFor(waiting_for));
                    }
                    UiEvent::SigningConfirm { phase } => {
                        ui.set_busy_task(ui::BusyTask::Signing);
                        outbox.extend(
                            signer
                                .sign_ack(*phase, &mut hmac_keys.share_encryption)
                                .expect("state changed while acking sign"),
                        );
                    }
                    UiEvent::NameConfirm(ref new_name) => {
                        name = Some(new_name.into());
                        mutation_log
                            .push(Mutation::Name(new_name.to_string()))
                            .expect("flash write fail");
                        ui.set_device_name(new_name.into());
                        upstream_connection.send_to_coordinator([DeviceSendBody::SetName {
                            name: new_name.into(),
                        }]);
                    }
                    UiEvent::BackupRequestConfirm { phase } => {
                        outbox.extend(
                            signer
                                .display_backup_ack(*phase, &mut hmac_keys.share_encryption)
                                .expect("state changed while displaying backup"),
                        );
                        upstream_connection.send_to_coordinator([DeviceSendBody::Misc(
                            CommsMisc::DisplayBackupConfrimed,
                        )]);
                    }
                    UiEvent::UpgradeConfirm => {
                        if let Some(upgrade) = upgrade.as_mut() {
                            upgrade.upgrade_confirm();
                        }
                        switch_workflow = None;
                    }
                    UiEvent::EnteredShareBackup {
                        phase,
                        share_backup,
                    } => {
                        outbox.extend(
                            signer.tell_coordinator_about_backup_load_result(phase, share_backup),
                        );
                    }
                    UiEvent::WipeDataConfirm => {
                        nvs.erase_all().expect("failed to erase nvs");
                        reset(&mut upstream_serial);
                    }
                }

                if let Some(switch_workflow) = switch_workflow {
                    ui.set_workflow(switch_workflow);
                }
            }
        }

        // Let UI redraw if needed
        if let Some(ui_event) = ui.poll() {
            ui_event_queue.push_back(ui_event);
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

fn reset<T>(upstream_serial: &mut SerialInterface<T, Upstream>)
where
    T: esp_hal::timer::Timer,
{
    let _ = upstream_serial.send_reset_signal();
    esp_hal::reset::software_reset();
}
