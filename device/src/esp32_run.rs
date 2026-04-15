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
use frostsnap_comms::{
    CommsMisc, CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceName, DeviceSendBody,
    ReceiveSerial, Sha256Digest, Upstream, MAGIC_BYTES_PERIOD,
};
use frostsnap_core::{
    device::{DeviceToUserMessage, FrostSigner},
    device_nonces::NonceJobBatch,
    message::{self, DeviceSend},
    DeviceId,
};
#[cfg(feature = "debug_log")]
use frostsnap_core::{Gist, Kind};
use frostsnap_embedded::NonceAbSlot;
use rand_core::RngCore;

use crate::ds::HardwareDs;
use crate::efuse::EfuseHmacKeys;
use crate::frosty_ui::FrostyUi;
use crate::ota::OtaPartitions;
use crate::partitions::EspFlashPartition;
use esp_hal::{
    gpio::{AnyPin, Input},
    peripherals::TIMG0,
    rsa::Rsa,
    sha::Sha,
    timer::{
        timg::{Timer as TimgTimer, Timer0},
        Timer as TimerTrait,
    },
    Blocking,
};
use frostsnap_comms::Downstream;
use rand_chacha::ChaCha20Rng;

type EspSerial<'a, D> = SerialInterface<'a, TimgTimer<Timer0<TIMG0>, Blocking>, D>;
type EspMutationLog<'a> = MutationLog<'a, esp_storage::FlashStorage>;
type EspSigner<'a> = FrostSigner<NonceAbSlot<'a, esp_storage::FlashStorage>>;

struct DeviceLoop<'a> {
    // Borrowed from Resources
    rng: &'a mut ChaCha20Rng,
    hmac_keys: &'a mut EfuseHmacKeys<'a>,
    hardware_rsa: &'a mut Option<HardwareDs<'a>>,
    certificate: &'a Option<frostsnap_comms::genuine_certificate::Certificate>,
    ota_partitions: &'a mut OtaPartitions<'a>,
    ui: &'a mut FrostyUi<'a>,
    timer: &'a TimgTimer<Timer0<TIMG0>, Blocking>,
    sha256: &'a mut Sha<'a>,
    upstream_serial: &'a mut EspSerial<'a, Upstream>,
    downstream_serial: &'a mut EspSerial<'a, Downstream>,
    downstream_detect: &'a mut Input<'a, AnyPin>,
    rsa: &'a mut Rsa<'a, Blocking>,

    // Owned values created during init
    full_nvs: EspFlashPartition<'a>,
    mutation_log: EspMutationLog<'a>,
    signer: EspSigner<'a>,
    name: Option<DeviceName>,
    device_id: DeviceId,
    active_firmware_digest: Sha256Digest,
    upstream_connection: UpstreamConnection,

    // Mutable loop state
    soft_reset: bool,
    downstream_connection_state: DownstreamConnectionState,
    outbox: VecDeque<DeviceSend>,
    nonce_task_batch: Option<NonceJobBatch>,
    inbox: Vec<CoordinatorSendBody>,
    next_write_magic_bytes_downstream: Instant,
    magic_bytes_timeout_counter: u32,
    upgrade: Option<ota::FirmwareUpgradeMode<'a>>,
    erase_state: Option<erase::Erase>,
    pending_device_name: Option<DeviceName>,
}

impl<'a> DeviceLoop<'a> {
    /// Box in here rather than the caller so LLVM can construct directly into the heap
    /// allocation and avoid putting the struct on the caller's stack.
    #[inline(never)]
    fn new(resources: &'a mut Resources<'a>) -> Box<Self> {
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

        let full_nvs = *nvs;

        let header_sectors = nvs.split_off_front(2);
        let header_flash = crate::flash::FlashHeader::new(header_sectors);
        let header = match header_flash.read_header() {
            Some(h) => h,
            None => {
                if !nvs.is_empty().expect("checking NVS is empty") {
                    let mut erase_op = erase::Erase::new(&full_nvs);
                    while !matches!(erase_op.poll(&full_nvs, ui), erase::ErasePoll::Reset) {}
                    esp_hal::reset::software_reset();
                }
                header_flash.init(rng)
            }
        };
        let device_keypair = header.device_keypair(&mut hmac_keys.fixed_entropy);

        let share_partition = nvs.split_off_front(2);

        // Keep some space reserved for other potential uses in the future, 8 AB slots
        let _reserved = nvs.split_off_front(8 * 2);

        let nonce_slots = {
            let mut n_nonce_sectors = nvs.n_sectors().div_ceil(2);
            n_nonce_sectors = (n_nonce_sectors.div_ceil(2) * 2).max(16);
            NonceAbSlot::load_slots(nvs.split_off_front(n_nonce_sectors))
        };

        let mut mutation_log = MutationLog::new(share_partition, *nvs);
        let mut signer = FrostSigner::new(device_keypair, nonce_slots);

        let mut name: Option<DeviceName> = None;
        for change in mutation_log.seek_iter() {
            match change {
                Ok(change) => match change {
                    Mutation::Core(mutation) => {
                        signer.apply_mutation(mutation);
                    }
                    Mutation::Name(name_update) => {
                        let device_name = frostsnap_comms::DeviceName::truncate(name_update);
                        name = Some(device_name);
                    }
                },
                Err(e) => {
                    panic!("failed to read event: {e}");
                }
            }
        }

        let active_partition = ota_partitions.active_partition();
        let (firmware_size, _firmware_and_signature_block_size) =
            active_partition.firmware_size().unwrap();
        let active_firmware_digest = active_partition.sha256_digest(sha256, Some(firmware_size));

        let device_id = signer.device_id();

        let workflow = match (name.as_ref(), signer.held_shares().next()) {
            (Some(device_name), Some(held_share)) => ui::Workflow::Standby {
                device_name: device_name.clone(),
                held_share,
            },
            _ => ui::Workflow::None,
        };
        ui.set_default_workflow(workflow);
        ui.go_to_default();

        let upstream_connection = UpstreamConnection::new(device_id);
        ui.set_upstream_connection_state(upstream_connection.state);
        ui.clear_busy_task();

        Box::new(Self {
            rng,
            hmac_keys,
            hardware_rsa,
            certificate,
            ota_partitions,
            ui,
            timer,
            sha256,
            upstream_serial,
            downstream_serial,
            downstream_detect,
            rsa,
            full_nvs,
            mutation_log,
            signer,
            name,
            device_id,
            active_firmware_digest,
            upstream_connection,
            soft_reset: true,
            downstream_connection_state: DownstreamConnectionState::Disconnected,
            outbox: VecDeque::new(),
            nonce_task_batch: None,
            inbox: vec![],
            next_write_magic_bytes_downstream: Instant::from_ticks(0),
            magic_bytes_timeout_counter: 0,
            upgrade: None,
            erase_state: None,
            pending_device_name: None,
        })
    }

    fn update_default_workflow(&mut self) {
        let workflow = match (self.name.as_ref(), self.signer.held_shares().next()) {
            (Some(device_name), Some(held_share)) => ui::Workflow::Standby {
                device_name: device_name.clone(),
                held_share,
            },
            _ => ui::Workflow::None,
        };
        self.ui.set_default_workflow(workflow);
    }

    fn save_pending_device_name(&mut self) -> bool {
        let Some(new_name) = self.pending_device_name.take() else {
            return false;
        };
        self.name = Some(new_name.clone());
        self.mutation_log
            .push(Mutation::Name(new_name.to_string()))
            .expect("flash write fail");
        self.upstream_connection
            .send_to_coordinator([DeviceSendBody::SetName { name: new_name }]);
        true
    }

    #[inline(never)]
    fn poll(&mut self) {
        if self.soft_reset {
            self.soft_reset = false;
            self.magic_bytes_timeout_counter = 0;
            self.signer.clear_tmp_data();
            self.downstream_connection_state = DownstreamConnectionState::Disconnected;
            self.upstream_connection
                .set_state(UpstreamConnectionState::PowerOn, self.ui);
            self.next_write_magic_bytes_downstream = Instant::from_ticks(0);
            self.update_default_workflow();
            self.ui.go_to_default();
            self.upgrade = None;
            self.pending_device_name = None;
            self.outbox.clear();
            self.nonce_task_batch = None;
        }

        let is_usb_connected_downstream = !self.downstream_detect.is_high();

        // === DOWNSTREAM connection management
        match (
            is_usb_connected_downstream,
            self.downstream_connection_state,
        ) {
            (true, DownstreamConnectionState::Disconnected) => {
                self.downstream_connection_state = DownstreamConnectionState::Connected;
                self.ui
                    .set_downstream_connection_state(self.downstream_connection_state);
            }
            (true, DownstreamConnectionState::Connected) => {
                let now = self.timer.now();
                if now > self.next_write_magic_bytes_downstream {
                    self.next_write_magic_bytes_downstream = now
                        .checked_add_duration(Duration::millis(MAGIC_BYTES_PERIOD))
                        .expect("won't overflow");
                    self.downstream_serial
                        .write_magic_bytes()
                        .expect("couldn't write magic bytes downstream");
                }
                if self.downstream_serial.find_and_remove_magic_bytes() {
                    self.downstream_connection_state = DownstreamConnectionState::Established;
                    self.ui
                        .set_downstream_connection_state(self.downstream_connection_state);
                    self.upstream_connection
                        .send_debug("Device read magic bytes from another device!");
                }
            }
            (
                false,
                state @ DownstreamConnectionState::Established
                | state @ DownstreamConnectionState::Connected,
            ) => {
                self.downstream_connection_state = DownstreamConnectionState::Disconnected;
                self.ui
                    .set_downstream_connection_state(self.downstream_connection_state);
                if state == DownstreamConnectionState::Established {
                    self.upstream_connection
                        .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
                }
            }
            _ => { /* nothing to do */ }
        }

        if self.downstream_connection_state == DownstreamConnectionState::Established {
            while let Some(device_send) = self.downstream_serial.receive() {
                match device_send {
                    Ok(device_send) => {
                        match device_send {
                            ReceiveSerial::MagicBytes(_) => {
                                self.upstream_connection
                                    .send_debug("downstream device sent unexpected magic bytes");
                                self.upstream_connection
                                    .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
                                self.downstream_connection_state =
                                    DownstreamConnectionState::Disconnected;
                            }
                            ReceiveSerial::Message(message) => {
                                self.upstream_connection.forward_to_coordinator(message);
                            }
                            ReceiveSerial::Conch => { /* deprecated */ }
                            ReceiveSerial::Reset => {
                                self.upstream_connection
                                    .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
                                self.downstream_connection_state =
                                    DownstreamConnectionState::Disconnected;
                                break;
                            }
                            _ => { /* unused */ }
                        };
                    }
                    Err(e) => {
                        self.upstream_connection
                            .send_debug(format!("Failed to decode on downstream port: {e}"));
                        self.upstream_connection
                            .send_to_coordinator([DeviceSendBody::DisconnectDownstream]);
                        self.downstream_connection_state = DownstreamConnectionState::Disconnected;
                        break;
                    }
                };
            }
        }

        // === UPSTREAM connection management
        match self.upstream_connection.get_state() {
            UpstreamConnectionState::PowerOn => {
                if self.upstream_serial.find_and_remove_magic_bytes() {
                    self.upstream_serial
                        .write_magic_bytes()
                        .expect("failed to write magic bytes");
                    log_and_redraw!(self.ui, "upstream got magic bytes");

                    self.upstream_connection
                        .send_announcement(DeviceSendBody::Announce {
                            firmware_digest: self.active_firmware_digest,
                        });
                    self.upstream_connection
                        .send_to_coordinator([match &self.name {
                            Some(name) => DeviceSendBody::SetName { name: name.clone() },
                            None => DeviceSendBody::NeedName,
                        }]);

                    self.upstream_connection
                        .set_state(UpstreamConnectionState::Established, self.ui);
                }
            }
            _ => {
                let mut last_message_was_magic_bytes = false;
                while let Some(received_message) = self.upstream_serial.receive() {
                    match received_message {
                        Ok(received_message) => {
                            let received_message: ReceiveSerial<Upstream> = received_message;
                            last_message_was_magic_bytes =
                                matches!(received_message, ReceiveSerial::MagicBytes(_));
                            match received_message {
                                ReceiveSerial::Message(mut message) => {
                                    let for_me: bool = message
                                        .target_destinations
                                        .remove_from_recipients(self.device_id);

                                    if self.downstream_connection_state
                                        == DownstreamConnectionState::Established
                                        && message.target_destinations.should_forward()
                                    {
                                        self.downstream_serial
                                            .send(message.clone())
                                            .expect("sending downstream");
                                    }

                                    if for_me {
                                        log_and_redraw!(
                                            self.ui,
                                            "RECV: {}",
                                            message.message_body.gist()
                                        );

                                        match message.message_body.decode() {
                                            Some(CoordinatorSendBody::Upgrade(
                                                CoordinatorUpgradeMessage::EnterUpgradeMode,
                                            )) => {
                                                if let Some(mut upgrade) = self.upgrade.take() {
                                                    let upstream_io =
                                                        self.upstream_serial.inner_mut();
                                                    upgrade.enter_upgrade_mode(
                                                        upstream_io,
                                                        if self.downstream_connection_state
                                                            == DownstreamConnectionState::Established
                                                        {
                                                            Some(
                                                                self.downstream_serial.inner_mut(),
                                                            )
                                                        } else {
                                                            None
                                                        },
                                                        self.ui,
                                                        self.sha256,
                                                        self.timer,
                                                        self.rsa,
                                                    );
                                                    reset(self.upstream_serial);
                                                } else {
                                                    panic!("upgrade cannot start because we were not warned about it")
                                                }
                                            }
                                            Some(decoded) => {
                                                self.inbox.push(decoded);
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
                    self.upstream_connection.get_state(),
                    UpstreamConnectionState::EstablishedAndCoordAck
                );

                if last_message_was_magic_bytes {
                    if is_upstream_established {
                        self.soft_reset = true;
                    } else if self.magic_bytes_timeout_counter > 2 {
                        // Coord is still in `awaiting_magic` long after we
                        // replied — it didn't see our Announce. Reset so we
                        // re-handshake on the next magic bytes. Threshold has
                        // a small margin because the coord may have queued a
                        // few magic-bytes frames during its initial retry loop
                        // before reading our reply (in-flight noise).
                        self.upstream_connection
                            .set_state(UpstreamConnectionState::PowerOn, self.ui);
                        self.magic_bytes_timeout_counter = 0;
                    } else {
                        self.magic_bytes_timeout_counter += 1;
                    }
                }

                if let Some(upgrade_) = &mut self.upgrade {
                    let message = upgrade_.poll(self.ui);
                    self.upstream_connection.send_to_coordinator(message);
                }

                if let Some(erase_) = &mut self.erase_state {
                    match erase_.poll(&self.full_nvs, self.ui) {
                        erase::ErasePoll::Pending => {}
                        erase::ErasePoll::SendConfirmation(msg) => {
                            self.upstream_connection.send_to_coordinator([*msg]);
                        }
                        erase::ErasePoll::Reset => {
                            reset(self.upstream_serial);
                        }
                    }
                }
            }
        }

        // Process inbox messages
        let mut inbox = core::mem::take(&mut self.inbox);
        for message_body in inbox.drain(..) {
            match &message_body {
                CoordinatorSendBody::Cancel => {
                    self.signer.clear_tmp_data();
                    self.ui.go_to_default();
                    self.pending_device_name = None;
                    self.upgrade = None;
                }
                CoordinatorSendBody::AnnounceAck => {
                    self.upstream_connection
                        .set_state(UpstreamConnectionState::EstablishedAndCoordAck, self.ui);
                }
                CoordinatorSendBody::Naming(naming) => match naming {
                    frostsnap_comms::NameCommand::Preview(preview_name) => {
                        self.pending_device_name = Some(preview_name.clone());
                        self.ui.set_workflow(ui::Workflow::NamingDevice {
                            new_name: preview_name.clone(),
                        });
                    }
                    frostsnap_comms::NameCommand::Prompt(new_name) => {
                        self.ui
                            .set_workflow(ui::Workflow::prompt(ui::Prompt::NewName {
                                old_name: self.name.clone(),
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
                        self.ui.set_busy_task(ui::BusyTask::GeneratingNonces);
                    } else {
                        self.ui.clear_busy_task();
                    }
                    log_and_redraw!(self.ui, "process: {}", core_message.kind());
                    self.outbox.extend(
                        self.signer
                            .recv_coordinator_message(core_message.clone(), self.rng)
                            .expect("failed to process coordinator message"),
                    );
                    log_and_redraw!(self.ui, "done");
                }
                CoordinatorSendBody::Upgrade(upgrade_message) => match upgrade_message {
                    CoordinatorUpgradeMessage::PrepareUpgrade {
                        size,
                        firmware_digest,
                    } => {
                        let upgrade_ = self.ota_partitions.start_upgrade(
                            *size,
                            *firmware_digest,
                            self.active_firmware_digest,
                        );
                        self.upgrade = Some(upgrade_);
                    }
                    CoordinatorUpgradeMessage::PrepareUpgrade2 {
                        size,
                        firmware_digest,
                    } => {
                        let upgrade_ = self.ota_partitions.start_upgrade(
                            *size,
                            *firmware_digest,
                            self.active_firmware_digest,
                        );
                        self.upgrade = Some(upgrade_);
                    }
                    CoordinatorUpgradeMessage::EnterUpgradeMode => {}
                },
                CoordinatorSendBody::DataErase => self
                    .ui
                    .set_workflow(ui::Workflow::prompt(ui::Prompt::EraseDevice)),
                CoordinatorSendBody::Challenge(challenge) => {
                    if let (Some(hw_rsa), Some(cert)) =
                        (self.hardware_rsa.as_mut(), self.certificate.as_ref())
                    {
                        let signature = hw_rsa.sign(&challenge.0, self.sha256);
                        self.upstream_connection.send_to_coordinator([
                            DeviceSendBody::SignedChallenge {
                                signature: Box::new(signature),
                                certificate: Box::new(cert.clone()),
                            },
                        ]);
                    }
                }
            }
        }
        self.inbox = inbox;

        // Apply any staged mutations
        {
            let staged_mutations = self.signer.staged_mutations();
            if !staged_mutations.is_empty() {
                let now = self.timer.now();
                self.mutation_log
                    .append(staged_mutations.drain(..).map(Mutation::Core))
                    .expect("writing core mutations failed");
                let after = self.timer.now().checked_duration_since(now).unwrap();
                self.upstream_connection
                    .send_debug(format!("core mutations took {}ms", after.to_millis()));
            }
        }

        // Poll nonce job batch
        if let Some(batch) = self.nonce_task_batch.as_mut() {
            log_and_redraw!(self.ui, "nonce batch start");
            if batch.do_work(&mut self.hmac_keys.share_encryption) {
                log_and_redraw!(self.ui, "nonce batch finish");
                let completed_batch = self.nonce_task_batch.take().unwrap();
                let segments = completed_batch.into_segments();
                self.outbox.push_back(DeviceSend::ToCoordinator(Box::new(
                    message::DeviceToCoordinatorMessage::Signing(
                        message::signing::DeviceSigning::NonceResponse { segments },
                    ),
                )));
            }
            log_and_redraw!(self.ui, "nonce batch done");
        }

        // Handle message outbox to send
        while let Some(send) = self.outbox.pop_front() {
            match send {
                DeviceSend::ToCoordinator(boxed) => {
                    self.upstream_connection
                        .send_to_coordinator([DeviceSendBody::Core(*boxed)]);
                }
                DeviceSend::ToUser(boxed) => {
                    match *boxed {
                        DeviceToUserMessage::FinalizeKeyGen { key_name: _ } => {
                            assert!(
                                self.save_pending_device_name(),
                                "must have named device before starting keygen"
                            );
                            self.ui.clear_busy_task();
                            self.update_default_workflow();
                            self.ui.go_to_default();
                        }
                        DeviceToUserMessage::CheckKeyGen { phase } => {
                            self.ui
                                .set_workflow(ui::Workflow::prompt(ui::Prompt::KeyGen { phase }));
                        }
                        DeviceToUserMessage::VerifyAddress {
                            address,
                            bip32_path,
                        } => {
                            let rand_seed = self.rng.next_u32();
                            self.ui.set_workflow(ui::Workflow::DisplayAddress {
                                address,
                                bip32_path,
                                rand_seed,
                            })
                        }
                        DeviceToUserMessage::SignatureRequest { phase } => {
                            let rand_seed = self.rng.next_u32();
                            self.ui
                                .set_workflow(ui::Workflow::prompt(ui::Prompt::Signing {
                                    phase,
                                    rand_seed,
                                }));
                        }
                        DeviceToUserMessage::Restoration(to_user_restoration) => {
                            use frostsnap_core::device::restoration::ToUserRestoration::*;
                            match *to_user_restoration {
                                DisplayBackup {
                                    key_name,
                                    access_structure_ref,
                                    phase,
                                } => {
                                    let backup = phase
                                        .decrypt_to_backup(&mut self.hmac_keys.share_encryption)
                                        .expect("state changed while displaying backup");
                                    self.ui.set_workflow(ui::Workflow::DisplayBackup {
                                        key_name: key_name.to_string(),
                                        backup,
                                        access_structure_ref,
                                    });
                                }
                                EnterBackup { phase } => {
                                    self.ui.set_workflow(ui::Workflow::EnteringBackup(phase));
                                }
                                BackupSaved { .. } => {
                                    assert!(
                                        self.save_pending_device_name(),
                                        "must have named device before loading backup"
                                    );
                                    self.update_default_workflow();
                                    self.ui.go_to_default();
                                }
                                CheckBackup {
                                    key_name,
                                    access_structure_ref,
                                    phase,
                                } => {
                                    let backup = phase
                                        .decrypt_to_backup(&mut self.hmac_keys.share_encryption)
                                        .expect("state changed while checking backup");
                                    let rand_seed = self.rng.next_u32();
                                    self.ui.set_workflow(ui::Workflow::CheckBackup {
                                        key_name: key_name.to_string(),
                                        backup,
                                        access_structure_ref,
                                        rand_seed,
                                    });
                                }
                                ConsolidateBackup(phase) => {
                                    // Auto-confirm: the user can't meaningfully verify this
                                    self.outbox.extend(self.signer.finish_consolidation(
                                        &mut self.hmac_keys.share_encryption,
                                        phase,
                                        self.rng,
                                    ));

                                    self.save_pending_device_name();
                                    self.update_default_workflow();
                                    self.ui.go_to_default();
                                }
                            }
                        }
                        DeviceToUserMessage::NonceJobs(batch) => {
                            self.nonce_task_batch = Some(batch);
                        }
                    };
                }
            }
        }

        // Handle UI events
        if let Some(ui_event) = self.ui.poll() {
            match ui_event {
                UiEvent::KeyGenConfirm { phase } => {
                    self.outbox.extend(
                        self.signer
                            .keygen_ack(*phase, &mut self.hmac_keys.share_encryption, self.rng)
                            .expect("state changed while confirming keygen"),
                    );
                    self.ui.clear_busy_task();
                }
                UiEvent::SigningConfirm { phase } => {
                    self.ui.set_busy_task(ui::BusyTask::Signing);
                    self.outbox.extend(
                        self.signer
                            .sign_ack(*phase, &mut self.hmac_keys.share_encryption)
                            .expect("state changed while acking sign"),
                    );
                }
                UiEvent::NameConfirm(ref new_name) => {
                    self.mutation_log
                        .push(Mutation::Name(new_name.to_string()))
                        .expect("flash write fail");
                    self.name = Some(new_name.clone());
                    self.update_default_workflow();
                    self.pending_device_name = Some(new_name.clone());
                    self.ui.set_workflow(ui::Workflow::NamingDevice {
                        new_name: new_name.clone(),
                    });
                    self.upstream_connection
                        .send_to_coordinator([DeviceSendBody::SetName {
                            name: new_name.clone(),
                        }]);
                }
                UiEvent::BackupRecorded => {
                    self.upstream_connection
                        .send_to_coordinator([DeviceSendBody::Misc(CommsMisc::BackupRecorded)]);
                }
                UiEvent::BackupChecked {
                    access_structure_ref,
                    share_index,
                } => {
                    self.upstream_connection
                        .send_to_coordinator([DeviceSendBody::Misc(CommsMisc::BackupChecked {
                            access_structure_ref,
                            share_index,
                        })]);
                }
                UiEvent::UpgradeConfirm => {
                    if let Some(upgrade) = self.upgrade.as_mut() {
                        upgrade.upgrade_confirm();
                    }
                }
                UiEvent::EnteredShareBackup {
                    phase,
                    share_backup,
                } => {
                    self.outbox.extend(
                        self.signer
                            .tell_coordinator_about_backup_load_result(phase, share_backup),
                    );
                    self.ui.go_to_default();
                }
                UiEvent::EraseDataConfirm => {
                    self.erase_state = Some(erase::Erase::new(&self.full_nvs));
                }
            }
        }

        if let Some(message) = self.upstream_connection.dequeue_message() {
            self.upstream_serial
                .send(message)
                .expect("failed to send message upstream");
        }
    }
}

/// Main event loop for the device
pub fn run<'a>(resources: &'a mut Resources<'a>) -> ! {
    let mut device_loop = DeviceLoop::new(resources);
    loop {
        device_loop.poll();
    }
}

fn reset<T>(upstream_serial: &mut SerialInterface<T, Upstream>)
where
    T: esp_hal::timer::Timer,
{
    let _ = upstream_serial.send_reset_signal();
    esp_hal::reset::software_reset();
}
