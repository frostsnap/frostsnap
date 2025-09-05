use crate::{
    efuse::EfuseHmacKeys,
    flash::{FlashHeader, Mutation, MutationLog},
    io::SerialInterface,
    ota,
    partitions::PartitionExt,
    ui::{self, UiEvent, UserInteraction, Workflow},
    DownstreamConnectionState, Duration, Instant, UpstreamConnection, UpstreamConnectionState,
};
use alloc::{
    collections::VecDeque,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::RefCell;
use esp_hal::{gpio, sha::Sha, timer};
use esp_storage::FlashStorage;
use frostsnap_comms::{
    CommsMisc, CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceSendBody, ReceiveSerial,
    Upstream,
};
use frostsnap_comms::{Downstream, MAGIC_BYTES_PERIOD};
use frostsnap_core::{
    device::{DeviceToUserMessage, FrostSigner},
    message::DeviceSend,
};
use frostsnap_widgets::debug::log;
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
    pub fn run(&mut self) -> ! {
        let Run {
            upstream_serial,
            downstream_serial,
            rng,
            ui,
            timer,
            downstream_detect,
            sha256,
            hmac_keys,
        } = self;

        ui.set_busy_task(ui::BusyTask::Loading);

        let flash = RefCell::new(FlashStorage::new());
        let partitions = crate::partitions::Partitions::load(&flash);
        let mut nvs_partition = partitions.nvs;
        let header_flash = FlashHeader::new(nvs_partition.split_off_front(2));
        let header = match header_flash.read_header() {
            Some(header) => header,
            None => {
                if !partitions.nvs.is_empty().expect("checking NVS is empty") {
                    panic!("the device appears to be new but the NVS is not blank. Maybe you're a developer who needs to manually erase the device?");
                }
                header_flash.init(rng)
            }
        };

        let share_partition = nvs_partition.split_off_front(2);

        let nonce_slots = {
            // give half the remaining nvs over to nonces
            let mut n_nonce_sectors = nvs_partition.n_sectors().div_ceil(2);
            // Make sure it's a multiple of 2
            n_nonce_sectors = (n_nonce_sectors.div_ceil(2) * 2).max(16 /* but at least 16 */);
            // each nonce slot requires 2 sectors so divide by 2 to get the number of slots
            frostsnap_embedded::NonceAbSlot::load_slots(
                nvs_partition.split_off_front(n_nonce_sectors),
            )
        };

        // The event log gets the reset of the sectors
        let mut mutation_log = MutationLog::new(share_partition, nvs_partition);

        // Since this keypair is created on startup, this call could prove weak to side-channel
        // attacks. Because device keypair compromise only affects device identity and keygen share
        //  encryption (rather than signing keys themselves), we choose to use the less critical
        // `fixed-entropy` HMAC for this purpose.
        let device_keypair = header.device_keypair(&mut hmac_keys.fixed_entropy);
        let mut signer = FrostSigner::new(device_keypair, nonce_slots);

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

        let active_partition = partitions.ota.active_partition();
        let firmware_size = active_partition.firmware_size().unwrap();
        let active_firmware_digest = active_partition.sha256_digest(sha256, Some(firmware_size));

        let device_id = signer.device_id();

        let mut soft_reset = true;
        let mut downstream_connection_state = DownstreamConnectionState::Disconnected;
        let mut sends_user: Vec<DeviceToUserMessage> = vec![];
        let mut outbox = VecDeque::new();
        let mut inbox: Vec<CoordinatorSendBody> = vec![];
        let mut next_write_magic_bytes_downstream: Instant = Instant::from_ticks(0);
        // If we keep getting magic bytes instead of getting a proper message we have to accept that
        // the upstream doesn't think we're awake yet and we should soft reset.
        let mut magic_bytes_timeout_counter = 0;

        let mut upstream_connection = UpstreamConnection::new(device_id);

        ui.set_upstream_connection_state(upstream_connection.state);
        let mut upgrade: Option<ota::FirmwareUpgradeMode> = None;
        let mut pending_device_name: Option<String> = None;

        macro_rules! default_workflow {
            ($name:expr, $signer:expr) => {
                match ($name.as_ref(), signer.held_shares().next()) {
                    (Some(device_name), Some(held_share)) => Workflow::Standby {
                        device_name: device_name.clone(),
                        held_share,
                    },
                    _ => Workflow::None,
                }
            };
        }

        ui.clear_busy_task();
        ui.set_downstream_connection_state(DownstreamConnectionState::Disconnected);

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
                                    // soft disconnect downstream device to reset it becasue it's
                                    // doing stuff we don't understand.
                                    upstream_connection.send_to_coordinator([
                                        DeviceSendBody::DisconnectDownstream,
                                    ]);
                                    downstream_connection_state =
                                        DownstreamConnectionState::Disconnected;
                                }
                                ReceiveSerial::Message(message) => {
                                    upstream_connection.forward_to_coordinator(message);
                                }
                                ReceiveSerial::Conch => {
                                    upstream_serial.write_conch().expect("write conch upstream");
                                }
                                ReceiveSerial::Reset => {
                                    upstream_connection.send_to_coordinator([
                                        DeviceSendBody::DisconnectDownstream,
                                    ]);
                                    downstream_connection_state =
                                        DownstreamConnectionState::Disconnected;
                                    break;
                                }
                                _ => { /* unused */ }
                            };
                        }
                        Err(e) => {
                            log(format!("failed to read downstream:\n{e}"));
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
                                // Do this here because it needs to be set to false if it was
                                // previously true.
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
                                                        upgrade.enter_upgrade_mode(
                                                            upstream_serial.inner_mut(),
                                                            if downstream_connection_state == DownstreamConnectionState::Established { Some(downstream_serial.inner_mut()) } else { None },
                                                            ui,
                                                            sha256,
                                                            *timer
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
                                    ReceiveSerial::Conch => {
                                        // conch is depreciated -- ignore and forward it along
                                        if downstream_connection_state
                                            == DownstreamConnectionState::Established
                                        {
                                            downstream_serial
                                                .write_conch()
                                                .expect("write conch downstream");
                                        } else {
                                            // we can't send it upstream since
                                            // we don' t know if it's valid
                                            // since we're not obeying it
                                        }
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
                        } else if magic_bytes_timeout_counter > 20 {
                            // We keep receving magic bytes so we reset the
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

            // Process all received messages immediately
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
                            pending_device_name = Some(preview_name.to_string());
                            ui.set_workflow(ui::Workflow::NamingDevice {
                                new_name: preview_name.to_string(),
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
                            let upgrade_ = partitions.ota.start_upgrade(
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
                    let now = timer.now();
                    // ⚠ Apply any mutations made to flash before outputting anything to user or to coordinator
                    mutation_log
                        .append(staged_mutations.drain(..).map(Mutation::Core))
                        .expect("writing core mutations failed");
                    let after = timer.now().checked_duration_since(now).unwrap();
                    upstream_connection
                        .send_debug(format!("core mutations took {}ms", after.to_millis()));
                }
            }

            // Handle message outbox to send: ToCoordinator, ToUser.
            // ⚠ pop_front ensures messages are sent in order.
            while let Some(send) = outbox.pop_front() {
                match send {
                    DeviceSend::ToCoordinator(boxed) => {
                        upstream_connection.send_to_coordinator([DeviceSendBody::Core(*boxed)]);
                    }
                    DeviceSend::ToUser(boxed) => {
                        match *boxed {
                            DeviceToUserMessage::FinalizeKeyGen { key_name: _ } => {
                                let new_name = pending_device_name.clone().expect(
                                    "must have set pending_device_name before starting keygen",
                                );
                                // Save the device's name now that it's finished
                                name = Some(new_name.clone());
                                mutation_log
                                    .push(Mutation::Name(new_name.clone()))
                                    .expect("flash write fail");
                                upstream_connection.send_to_coordinator([
                                    DeviceSendBody::SetName { name: new_name },
                                ]);
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
                                        ui.set_workflow(ui::Workflow::EnteringBackup(phase));
                                    }
                                    BackupSaved { .. } => {
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
                                        ui.set_workflow(default_workflow!(name, signer));
                                    }
                                }
                            }
                        };
                    }
                }
            }

            // Process UI events immediately
            if let Some(ui_event) = ui.poll() {
                match ui_event {
                    UiEvent::KeyGenConfirm { phase } => {
                        outbox.extend(
                            signer
                                .keygen_ack(*phase, &mut hmac_keys.share_encryption, rng)
                                .expect("state changed while confirming keygen"),
                        );
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
                        name = Some(new_name.to_string());
                        pending_device_name = Some(new_name.to_string());
                        ui.set_workflow(ui::Workflow::NamingDevice {
                            new_name: new_name.to_string(),
                        });
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
                        partitions.nvs.erase_all().expect("failed to erase nvs");
                        reset(upstream_serial);
                    }
                }
            }

            // Send any pending messages to coordinator
            if let Some(message) = upstream_connection.dequeue_message() {
                upstream_serial
                    .send(message)
                    .expect("failed to send message upstream");
            }
        }
    }
}

fn reset<T: timer::Timer>(upstream_serial: &mut SerialInterface<'_, T, Upstream>) {
    let _ = upstream_serial.send_reset_signal();
    esp_hal::reset::software_reset();
}
