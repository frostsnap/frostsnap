//! Main event loop for the device

use crate::partitions::PartitionExt;
use crate::{
    erase,
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
    CommsMisc, CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceName, DeviceSendBody,
    ReceiveSerial, Upstream, MAGIC_BYTES_PERIOD,
};
use frostsnap_core::{
    device::{DeviceToUserMessage, FrostSigner},
    device_nonces::NonceJobBatch,
    message::{self, DeviceSend},
};
#[cfg(feature = "debug_log")]
use frostsnap_core::{Gist, Kind};
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
        ref mut upstream_serial,
        ref mut downstream_serial,
        ref mut downstream_detect,
        ref mut rsa,
    } = resources;
    // create an unmolested copy first so we can erase it all
    let full_nvs = *nvs;
    // Read device header and keypair from NVS
    let header_sectors = nvs.split_off_front(2);
    let header_flash = crate::flash::FlashHeader::new(header_sectors);
    let header = match header_flash.read_header() {
        Some(h) => h,
        None => {
            if !nvs.is_empty().expect("checking NVS is empty") {
                // Header is blank but NVS has data — a previous erase was
                // interrupted (the header is erased first). Finish the job.
                let mut erase_op = erase::Erase::new(&full_nvs);
                while !matches!(erase_op.poll(&full_nvs, ui), erase::ErasePoll::Reset) {}
                esp_hal::reset::software_reset();
            }
            // Initialize new header with device keypair
            header_flash.init(rng)
        }
    };
    let device_keypair = header.device_keypair(&mut hmac_keys.fixed_entropy);

    // Set up NVS partitions for shares, nonces, and mutation log
    let share_partition = nvs.split_off_front(2);

    // Keep some space reserved for other potential uses in the future, 8 AB slots
    let _reserved = nvs.split_off_front(8 * 2);

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
    let mut name: Option<DeviceName> = None;
    for change in mutation_log.seek_iter() {
        match change {
            Ok(change) => match change {
                Mutation::Core(mutation) => {
                    signer.apply_mutation(mutation);
                }
                Mutation::Name(name_update) => {
                    // Truncate to DeviceName length when loading from flash
                    let device_name = frostsnap_comms::DeviceName::truncate(name_update);
                    name = Some(device_name);
                }
            },
            Err(e) => {
                panic!("failed to read event: {e}");
            }
        }
    }

    // Note: widgets handles recovery mode internally

    // Get active firmware information
    let active_partition = ota_partitions.active_partition();
    let (firmware_size, _firmware_and_signature_block_size) =
        active_partition.firmware_size().unwrap();
    let active_firmware_digest = active_partition.sha256_digest(sha256, Some(firmware_size));

    let device_id = signer.device_id();

    // Initialize state variables
    let mut soft_reset = true;
    let mut downstream_connection_state = DownstreamConnectionState::Disconnected;
    let mut sends_user: Vec<DeviceToUserMessage> = vec![];
    let mut outbox = VecDeque::new();
    let mut nonce_task_batch: Option<NonceJobBatch> = None;
    let mut inbox: Vec<CoordinatorSendBody> = vec![];
    let mut next_write_magic_bytes_downstream: Instant = Instant::from_ticks(0);
    let mut magic_bytes_timeout_counter = 0;

    // Define default workflow macro
    macro_rules! default_workflow {
        ($name:expr, $signer:expr) => {
            match ($name.as_ref(), $signer.held_shares().next()) {
                (Some(device_name), Some(held_share)) => ui::Workflow::Standby {
                    device_name: device_name.clone(),
                    held_share,
                },
                _ => ui::Workflow::None,
            }
        };
    }

    ui.set_workflow(default_workflow!(name, signer));

    let mut upstream_connection = UpstreamConnection::new(device_id);
    ui.set_upstream_connection_state(upstream_connection.state);
    let mut upgrade: Option<ota::FirmwareUpgradeMode> = None;
    let mut erase_state: Option<erase::Erase> = None;
    let mut pending_device_name: Option<frostsnap_comms::DeviceName> = None;

    ui.clear_busy_task();

    // Main event loop
    loop {
        if soft_reset {
            soft_reset = false;
            magic_bytes_timeout_counter = 0;
            signer.clear_tmp_data();
            sends_user.clear();
            downstream_connection_state = DownstreamConnectionState::Disconnected;
            upstream_connection.set_state(UpstreamConnectionState::PowerOn, ui);
            next_write_magic_bytes_downstream = Instant::from_ticks(0);
            ui.set_workflow(default_workflow!(name, signer));
            upgrade = None;
            pending_device_name = None;
            outbox.clear();
            nonce_task_batch = None;
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
                                upstream_connection.forward_to_coordinator(message);
                            }
                            ReceiveSerial::Conch => { /* deprecated */ }
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

        // === UPSTREAM connection management
        match upstream_connection.get_state() {
            UpstreamConnectionState::PowerOn => {
                if upstream_serial.find_and_remove_magic_bytes() {
                    upstream_serial
                        .write_magic_bytes()
                        .expect("failed to write magic bytes");
                    log_and_redraw!(ui, "upstream got magic bytes");

                    upstream_connection.send_announcement(DeviceSendBody::Announce {
                        firmware_digest: active_firmware_digest,
                    });
                    upstream_connection.send_to_coordinator([match &name {
                        Some(name) => DeviceSendBody::SetName { name: name.clone() },
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
                                        log_and_redraw!(
                                            ui,
                                            "RECV: {}",
                                            message.message_body.gist()
                                        );

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
                                                        *timer,
                                                        rsa,
                                                    );
                                                    reset(upstream_serial);
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
                                ReceiveSerial::Conch => {}
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

                if let Some(erase_) = &mut erase_state {
                    match erase_.poll(&full_nvs, ui) {
                        erase::ErasePoll::Pending => {}
                        erase::ErasePoll::SendConfirmation(msg) => {
                            upstream_connection.send_to_coordinator([*msg]);
                        }
                        erase::ErasePoll::Reset => {
                            reset(upstream_serial);
                        }
                    }
                }
            }
        }

        // Process inbox messages
        for message_body in inbox.drain(..) {
            match &message_body {
                CoordinatorSendBody::Cancel => {
                    signer.clear_tmp_data();
                    ui.set_workflow(default_workflow!(name, signer));
                    // This either resets to the previous name, or clears it (if prev name does
                    // not exist).
                    pending_device_name = None;
                    upgrade = None;
                }
                CoordinatorSendBody::AnnounceAck => {
                    upstream_connection
                        .set_state(UpstreamConnectionState::EstablishedAndCoordAck, ui);
                }
                CoordinatorSendBody::Naming(naming) => match naming {
                    frostsnap_comms::NameCommand::Preview(preview_name) => {
                        pending_device_name = Some(preview_name.clone());
                        ui.set_workflow(ui::Workflow::NamingDevice {
                            new_name: preview_name.clone(),
                        });
                    }
                    frostsnap_comms::NameCommand::Prompt(new_name) => {
                        ui.set_workflow(ui::Workflow::prompt(ui::Prompt::NewName {
                            old_name: name.clone(),
                            new_name: new_name.clone(),
                        }));
                    }
                },
                CoordinatorSendBody::Core(core_message) => {
                    if matches!(
                        core_message,
                        message::CoordinatorToDeviceMessage::Signing(
                            message::signing::CoordinatorSigning::OpenNonceStreams { .. }
                        )
                    ) {
                        ui.set_busy_task(ui::BusyTask::GeneratingNonces);
                    } else {
                        ui.clear_busy_task();
                    }
                    log_and_redraw!(ui, "process: {}", core_message.kind());
                    outbox.extend(
                        signer
                            .recv_coordinator_message(core_message.clone(), rng)
                            .expect("failed to process coordinator message"),
                    );
                    log_and_redraw!(ui, "done");
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
                    CoordinatorUpgradeMessage::PrepareUpgrade2 {
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
                CoordinatorSendBody::DataErase => {
                    ui.set_workflow(ui::Workflow::prompt(ui::Prompt::EraseDevice))
                }
                CoordinatorSendBody::Challenge(challenge) => {
                    // Can only respond if we have hardware RSA and certificate
                    if let (Some(hw_rsa), Some(cert)) =
                        (hardware_rsa.as_mut(), certificate.as_ref())
                    {
                        let signature = hw_rsa.sign(challenge.as_ref(), sha256);
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

        // 🎯 Poll nonce job batch - process one nonce per iteration
        if let Some(batch) = nonce_task_batch.as_mut() {
            log_and_redraw!(ui, "nonce batch start");
            if batch.do_work(&mut hmac_keys.share_encryption) {
                log_and_redraw!(ui, "nonce batch finish");
                // Batch completed, send the response with all segments
                let completed_batch = nonce_task_batch.take().unwrap();
                let segments = completed_batch.into_segments();
                outbox.push_back(DeviceSend::ToCoordinator(Box::new(
                    message::DeviceToCoordinatorMessage::Signing(
                        message::signing::DeviceSigning::NonceResponse { segments },
                    ),
                )));
            }
            log_and_redraw!(ui, "nonce batch done");
        }

        // Handle message outbox to send
        while let Some(send) = outbox.pop_front() {
            match send {
                DeviceSend::ToCoordinator(boxed) => {
                    upstream_connection.send_to_coordinator([DeviceSendBody::Core(*boxed)]);
                }
                DeviceSend::ToUser(boxed) => {
                    match *boxed {
                        DeviceToUserMessage::FinalizeKeyGen { key_name: _ } => {
                            assert!(
                                save_pending_device_name(
                                    &mut pending_device_name,
                                    &mut name,
                                    &mut mutation_log,
                                    &mut upstream_connection,
                                ),
                                "must have named device before starting keygen"
                            );
                            ui.clear_busy_task();
                            ui.set_workflow(default_workflow!(name, signer));
                        }
                        DeviceToUserMessage::CheckKeyGen { phase } => {
                            ui.set_workflow(ui::Workflow::prompt(ui::Prompt::KeyGen { phase }));
                        }
                        DeviceToUserMessage::VerifyAddress {
                            address,
                            bip32_path,
                        } => {
                            let rand_seed = rng.next_u32();
                            ui.set_workflow(ui::Workflow::DisplayAddress {
                                address,
                                bip32_path,
                                rand_seed,
                            })
                        }
                        DeviceToUserMessage::SignatureRequest { phase } => {
                            ui.set_workflow(ui::Workflow::prompt(ui::Prompt::Signing { phase }));
                        }
                        DeviceToUserMessage::Restoration(to_user_restoration) => {
                            use frostsnap_core::device::restoration::ToUserRestoration::*;
                            match *to_user_restoration {
                                // Note: We immediately decrypt and display the backup without prompting.
                                // The coordinator has already requested this on behalf of the user.
                                // If we want to add "confirm before showing" in the future, we'd just
                                // delay calling phase.decrypt_to_backup() until after user confirms.
                                DisplayBackup {
                                    key_name,
                                    access_structure_ref,
                                    phase,
                                } => {
                                    let backup = phase
                                        .decrypt_to_backup(&mut hmac_keys.share_encryption)
                                        .expect("state changed while displaying backup");
                                    ui.set_workflow(ui::Workflow::DisplayBackup {
                                        key_name: key_name.to_string(),
                                        backup,
                                        access_structure_ref,
                                    });
                                }
                                EnterBackup { phase } => {
                                    ui.set_workflow(ui::Workflow::EnteringBackup(phase));
                                }
                                BackupSaved { .. } => {
                                    assert!(
                                        save_pending_device_name(
                                            &mut pending_device_name,
                                            &mut name,
                                            &mut mutation_log,
                                            &mut upstream_connection,
                                        ),
                                        "must have named device before loading backup"
                                    );
                                    ui.set_workflow(default_workflow!(name, signer));
                                }
                                ConsolidateBackup(phase) => {
                                    // XXX: We don't tell the user about this message and just automatically confirm it.
                                    // There isn't really anything they could do to actually verify to confirm it but since
                                    outbox.extend(signer.finish_consolidation(
                                        &mut hmac_keys.share_encryption,
                                        phase,
                                        rng,
                                    ));

                                    // The device can have a pending device name here if it was asked to consolidate right away instead of being asked to first save the backup.
                                    save_pending_device_name(
                                        &mut pending_device_name,
                                        &mut name,
                                        &mut mutation_log,
                                        &mut upstream_connection,
                                    );
                                    ui.set_workflow(default_workflow!(name, signer));
                                }
                            }
                        }
                        DeviceToUserMessage::NonceJobs(batch) => {
                            // 🚀 Set the batch for processing
                            nonce_task_batch = Some(batch);
                        }
                    };
                }
            }
        }

        // Handle UI events
        if let Some(ui_event) = ui.poll() {
            match ui_event {
                UiEvent::KeyGenConfirm { phase } => {
                    outbox.extend(
                        signer
                            .keygen_ack(*phase, &mut hmac_keys.share_encryption, rng)
                            .expect("state changed while confirming keygen"),
                    );
                    ui.clear_busy_task();
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
                    mutation_log
                        .push(Mutation::Name(new_name.to_string()))
                        .expect("flash write fail");
                    name = Some(new_name.clone());
                    pending_device_name = Some(new_name.clone());
                    ui.set_workflow(ui::Workflow::NamingDevice {
                        new_name: new_name.clone(),
                    });
                    upstream_connection.send_to_coordinator([DeviceSendBody::SetName {
                        name: new_name.clone(),
                    }]);
                }
                UiEvent::BackupRecorded {
                    access_structure_ref: _,
                } => {
                    upstream_connection
                        .send_to_coordinator([DeviceSendBody::Misc(CommsMisc::BackupRecorded)]);
                }
                UiEvent::UpgradeConfirm => {
                    if let Some(upgrade) = upgrade.as_mut() {
                        upgrade.upgrade_confirm();
                    }
                }
                UiEvent::EnteredShareBackup {
                    phase,
                    share_backup,
                } => {
                    outbox.extend(
                        signer.tell_coordinator_about_backup_load_result(phase, share_backup),
                    );
                }
                UiEvent::EraseDataConfirm => {
                    erase_state = Some(erase::Erase::new(&full_nvs));
                }
            }
        }

        if let Some(message) = upstream_connection.dequeue_message() {
            upstream_serial
                .send(message)
                .expect("failed to send message upstream");
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

/// Save a pending device name to flash and notify the coordinator
/// Returns true if a pending name was saved, false if there was no pending name
fn save_pending_device_name<S>(
    pending_device_name: &mut Option<DeviceName>,
    name: &mut Option<DeviceName>,
    mutation_log: &mut MutationLog<S>,
    upstream_connection: &mut UpstreamConnection,
) -> bool
where
    S: embedded_storage::nor_flash::NorFlash,
{
    let Some(new_name) = pending_device_name.take() else {
        return false;
    };
    *name = Some(new_name.clone());
    mutation_log
        .push(Mutation::Name(new_name.to_string()))
        .expect("flash write fail");
    upstream_connection.send_to_coordinator([DeviceSendBody::SetName { name: new_name }]);
    true
}
