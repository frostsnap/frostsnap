//! The portable device run-loop, lifted off esp-hal. `device/` builds a
//! `DeviceHal`/`FirmwareServices` over esp peripherals and a sim builds one over
//! in-memory peripherals; everything in here only ever sees the HAL traits.
//!
//! The esp-specific shell (`device::esp32_run`) owns the `DeviceLoop`, reads the
//! downstream-detect pin each tick, calls `poll`, and performs the hardware reset
//! when `poll` returns `Poll::ResetRequested`.

use crate::device_hal::{
    Clock, DeviceHal, FirmwareAction, FirmwareServices, HalParts, InitOutcome, Poll,
};
use crate::erase::{self, Erase};
use crate::flash_header::FlashHeader;
use crate::flash_log::{Mutation, MutationLog};
use crate::framed_serial::SerialPort;
use crate::ui::{self, UiEvent, UserInteraction};
use crate::{
    DownstreamConnectionState, FlashPartition, NonceAbSlot, UpstreamConnection,
    UpstreamConnectionState,
};
use alloc::{boxed::Box, collections::VecDeque, string::ToString, vec::Vec};
use frostsnap_comms::{
    CommsMisc, CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceName, DeviceSendBody,
    ReceiveSerial, Upstream, MAGIC_BYTES_PERIOD,
};
use frostsnap_core::{
    device::{DeviceToUserMessage, FrostSigner},
    device_nonces::NonceJobBatch,
    message::{self, DeviceSend},
    DeviceId, KeyId,
};
#[cfg(feature = "debug_log")]
use frostsnap_core::{Gist, Kind};
use rand_core::RngCore;

macro_rules! log {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug_log")]
        frostsnap_widgets::debug::log(alloc::format!($($arg)*))
    };
}

macro_rules! log_and_redraw {
    ($ui:expr, $($arg:tt)*) => {{
        log!($($arg)*);
        #[cfg(feature = "debug_log")]
        $ui.force_redraw();
    }};
}

/// Max consecutive magic-bytes frames tolerated after we've replied to the
/// coord but before it ack's. Absorbs the coord's `awaiting_magic` retry loop,
/// which may queue several frames before reading our reply (in-flight noise).
/// At the 100 ms coord cadence this is ~1 s before we give up and re-handshake.
const MAGIC_BYTES_RESET_THRESHOLD: u32 = 9;

type Signer<'a, S> = FrostSigner<NonceAbSlot<'a, S>>;

pub struct DeviceLoop<'a, H: DeviceHal, U: UserInteraction> {
    // Split-borrow of the HAL peripherals + the UI + the loop's own clock.
    parts: HalParts<'a, H>,
    ui: &'a mut U,
    clock: &'a dyn Clock,

    // Owned values created during init.
    full_nvs: FlashPartition<'a, H::Storage>,
    mutation_log: MutationLog<'a, H::Storage>,
    signer: Signer<'a, H::Storage>,
    name: Option<DeviceName>,
    device_id: DeviceId,
    upstream_connection: UpstreamConnection,

    // Mutable loop state.
    soft_reset: bool,
    downstream_connection_state: DownstreamConnectionState,
    outbox: VecDeque<DeviceSend>,
    nonce_task_batch: Option<NonceJobBatch>,
    inbox: Vec<CoordinatorSendBody>,
    next_write_magic_bytes_downstream_ms: u64,
    magic_bytes_timeout_counter: u32,
    erase_state: Option<Erase>,
    pending_device_name: Option<DeviceName>,
}

impl<'a, H: DeviceHal, U: UserInteraction> DeviceLoop<'a, H, U> {
    /// Box in here rather than the caller so LLVM can construct directly into the
    /// heap allocation and avoid putting the struct on the caller's stack.
    ///
    /// Returns `ResetRequested` if the init-time recovery erase ran (a header was
    /// missing but NVS wasn't blank); the shell resets instead of running.
    #[inline(never)]
    pub fn new(
        hal: &'a mut H,
        ui: &'a mut U,
        clock: &'a dyn Clock,
        mut nvs: FlashPartition<'a, H::Storage>,
    ) -> InitOutcome<Box<Self>> {
        let full_nvs = nvs;

        let header_sectors = nvs.split_off_front(2);
        let header_flash = FlashHeader::new(header_sectors);
        let header = match header_flash.read_header() {
            Some(h) => h,
            None => {
                if !nvs.is_empty().expect("checking NVS is empty") {
                    let mut erase_op = Erase::new(&full_nvs);
                    while !matches!(erase_op.poll(&full_nvs, ui), erase::ErasePoll::Reset) {}
                    return InitOutcome::ResetRequested;
                }
                header_flash.init(hal.parts().rng)
            }
        };
        let device_keypair = header.device_keypair(hal.keypair_hasher());

        let share_partition = nvs.split_off_front(2);

        // Keep some space reserved for other potential uses in the future, 8 AB slots
        let _reserved = nvs.split_off_front(8 * 2);

        let nonce_slots = {
            let mut n_nonce_sectors = nvs.n_sectors().div_ceil(2);
            n_nonce_sectors = (n_nonce_sectors.div_ceil(2) * 2).max(16);
            NonceAbSlot::load_slots(nvs.split_off_front(n_nonce_sectors))
        };

        let mut mutation_log = MutationLog::new(share_partition, nvs);
        let mut signer = FrostSigner::new(device_keypair, nonce_slots);

        let mut name: Option<DeviceName> = None;
        for change in mutation_log.seek_iter() {
            match change {
                Ok(Mutation::Core(mutation)) => {
                    signer.apply_mutation(mutation);
                }
                Ok(Mutation::Name(name_update)) => {
                    name = Some(DeviceName::truncate(name_update));
                }
                Err(e) => {
                    panic!("failed to read event: {e}");
                }
            }
        }

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
        ui.set_upstream_connection_state(upstream_connection.get_state());
        ui.clear_busy_task();

        InitOutcome::Ready(Box::new(Self {
            parts: hal.parts(),
            ui,
            clock,
            full_nvs,
            mutation_log,
            signer,
            name,
            device_id,
            upstream_connection,
            soft_reset: true,
            downstream_connection_state: DownstreamConnectionState::Disconnected,
            outbox: VecDeque::new(),
            nonce_task_batch: None,
            inbox: vec![],
            next_write_magic_bytes_downstream_ms: 0,
            magic_bytes_timeout_counter: 0,
            erase_state: None,
            pending_device_name: None,
        }))
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

    /// Send the upstream reset signal and ask the shell to reset the hardware.
    fn request_reset(&mut self) -> Poll {
        let _ = self.parts.upstream.send_reset_signal();
        Poll::ResetRequested
    }

    /// One tick of the run loop. `downstream_present` is the (already normalized)
    /// downstream-detect reading, taken by the shell before calling.
    #[inline(never)]
    pub fn poll(&mut self, downstream_present: bool) -> Poll {
        if self.soft_reset {
            self.soft_reset = false;
            self.magic_bytes_timeout_counter = 0;
            self.signer.clear_tmp_data();
            self.downstream_connection_state = DownstreamConnectionState::Disconnected;
            self.upstream_connection
                .set_state(UpstreamConnectionState::PowerOn, self.ui);
            self.next_write_magic_bytes_downstream_ms = 0;
            self.update_default_workflow();
            self.ui.go_to_default();
            self.parts.firmware.cancel();
            self.pending_device_name = None;
            self.outbox.clear();
            self.nonce_task_batch = None;
        }

        // === DOWNSTREAM connection management
        match (downstream_present, self.downstream_connection_state) {
            (true, DownstreamConnectionState::Disconnected) => {
                self.downstream_connection_state = DownstreamConnectionState::Connected;
                self.ui
                    .set_downstream_connection_state(self.downstream_connection_state);
            }
            (true, DownstreamConnectionState::Connected) => {
                let now_ms = self.clock.now_ms();
                if now_ms > self.next_write_magic_bytes_downstream_ms {
                    self.next_write_magic_bytes_downstream_ms = now_ms + MAGIC_BYTES_PERIOD;
                    self.parts
                        .downstream
                        .write_magic_bytes()
                        .expect("couldn't write magic bytes downstream");
                }
                if self.parts.downstream.find_and_remove_magic_bytes() {
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
            while let Some(device_send) = self.parts.downstream.receive() {
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
                if self.parts.upstream.find_and_remove_magic_bytes() {
                    self.parts
                        .upstream
                        .write_magic_bytes()
                        .expect("failed to write magic bytes");
                    log_and_redraw!(self.ui, "upstream got magic bytes");

                    let firmware_digest = self.parts.firmware.firmware_digest();
                    self.upstream_connection
                        .send_announcement(DeviceSendBody::Announce { firmware_digest });
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
                while let Some(received_message) = self.parts.upstream.receive() {
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
                                        self.parts
                                            .downstream
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
                                                let established = self.downstream_connection_state
                                                    == DownstreamConnectionState::Established;
                                                let action = {
                                                    let downstream = if established {
                                                        Some(&mut *self.parts.downstream)
                                                    } else {
                                                        None
                                                    };
                                                    self.parts.firmware.handle(
                                                        &CoordinatorSendBody::Upgrade(
                                                            CoordinatorUpgradeMessage::EnterUpgradeMode,
                                                        ),
                                                        self.parts.upstream,
                                                        downstream,
                                                        self.ui,
                                                    )
                                                };
                                                if matches!(action, FirmwareAction::ResetRequested)
                                                {
                                                    return self.request_reset();
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
                    } else if self.magic_bytes_timeout_counter > MAGIC_BYTES_RESET_THRESHOLD {
                        // Coord is still in `awaiting_magic` long after we
                        // replied — it didn't see our Announce. Reset so we
                        // re-handshake on the next magic bytes. See
                        // `MAGIC_BYTES_RESET_THRESHOLD` for the in-flight-noise
                        // margin rationale.
                        self.upstream_connection
                            .set_state(UpstreamConnectionState::PowerOn, self.ui);
                        self.magic_bytes_timeout_counter = 0;
                    } else {
                        self.magic_bytes_timeout_counter += 1;
                    }
                }

                match self.parts.firmware.poll(self.ui) {
                    FirmwareAction::Send(body) => {
                        self.upstream_connection.send_to_coordinator([*body]);
                    }
                    FirmwareAction::ResetRequested => {
                        return self.request_reset();
                    }
                    FirmwareAction::None => {}
                }

                if let Some(erase_) = &mut self.erase_state {
                    match erase_.poll(&self.full_nvs, self.ui) {
                        erase::ErasePoll::Pending => {}
                        erase::ErasePoll::SendConfirmation(msg) => {
                            self.upstream_connection.send_to_coordinator([*msg]);
                        }
                        erase::ErasePoll::Reset => {
                            return self.request_reset();
                        }
                    }
                }
            }
        }

        // Process inbox messages
        let mut inbox = core::mem::take(&mut self.inbox);
        let mut reset_after_inbox = false;
        for message_body in inbox.drain(..) {
            match &message_body {
                CoordinatorSendBody::Cancel => {
                    self.signer.clear_tmp_data();
                    self.ui.go_to_default();
                    self.pending_device_name = None;
                    self.parts.firmware.cancel();
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
                    // Retired rename command, kept only to reserve its wire slot.
                    frostsnap_comms::NameCommand::_Prompt(_) => {}
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
                            .recv_coordinator_message(core_message.clone(), self.parts.rng)
                            .expect("failed to process coordinator message"),
                    );
                    log_and_redraw!(self.ui, "done");
                }
                CoordinatorSendBody::Upgrade(_) | CoordinatorSendBody::Challenge(_) => {
                    let established =
                        self.downstream_connection_state == DownstreamConnectionState::Established;
                    let action = {
                        let downstream = if established {
                            Some(&mut *self.parts.downstream)
                        } else {
                            None
                        };
                        self.parts.firmware.handle(
                            &message_body,
                            self.parts.upstream,
                            downstream,
                            self.ui,
                        )
                    };
                    match action {
                        FirmwareAction::Send(body) => {
                            self.upstream_connection.send_to_coordinator([*body]);
                        }
                        FirmwareAction::ResetRequested => {
                            reset_after_inbox = true;
                            break;
                        }
                        FirmwareAction::None => {}
                    }
                }
                CoordinatorSendBody::DataErase => self
                    .ui
                    .set_workflow(ui::Workflow::prompt(ui::Prompt::EraseDevice)),
            }
        }
        self.inbox = inbox;
        if reset_after_inbox {
            return self.request_reset();
        }

        // Apply any staged mutations
        {
            let staged_mutations = self.signer.staged_mutations();
            if !staged_mutations.is_empty() {
                let started_ms = self.clock.now_ms();
                self.mutation_log
                    .append(staged_mutations.drain(..).map(Mutation::Core))
                    .expect("writing core mutations failed");
                let elapsed_ms = self.clock.now_ms().saturating_sub(started_ms);
                self.upstream_connection
                    .send_debug(format!("core mutations took {}ms", elapsed_ms));
            }
        }

        // Poll nonce job batch
        if let Some(batch) = self.nonce_task_batch.as_mut() {
            log_and_redraw!(self.ui, "nonce batch start");
            if batch.do_work(self.parts.secrets) {
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
                            let rand_seed = self.parts.rng.next_u32();
                            self.ui.set_workflow(ui::Workflow::DisplayAddress {
                                address,
                                bip32_path,
                                rand_seed,
                            })
                        }
                        DeviceToUserMessage::SignatureRequest { phase } => {
                            let rand_seed = self.parts.rng.next_u32();
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
                                        .decrypt_to_backup(self.parts.secrets)
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
                                        .decrypt_to_backup(self.parts.secrets)
                                        .expect("state changed while checking backup");
                                    let rand_seed = self.parts.rng.next_u32();
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
                                        self.parts.secrets,
                                        phase,
                                        self.parts.rng,
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
                            .keygen_ack(*phase, self.parts.secrets, self.parts.rng)
                            .expect("state changed while confirming keygen"),
                    );
                    self.ui.clear_busy_task();
                }
                UiEvent::SigningConfirm { phase } => {
                    self.ui.set_busy_task(ui::BusyTask::Signing);
                    self.outbox.extend(
                        self.signer
                            .sign_ack(*phase, self.parts.secrets)
                            .expect("state changed while acking sign"),
                    );
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
                    self.parts.firmware.confirm_upgrade();
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
                    self.erase_state = Some(Erase::new(&self.full_nvs));
                }
            }
        }

        if let Some(message) = self.upstream_connection.dequeue_message() {
            self.parts
                .upstream
                .send(message)
                .expect("failed to send message upstream");
        }

        Poll::Continue
    }

    /// Whether the device holds a finalized key for `key_id` (read-only, for
    /// sim/host harnesses to confirm a keygen actually persisted on the device).
    pub fn holds_key(&self, key_id: KeyId) -> bool {
        self.signer.wallet_network(key_id).is_some()
    }

    /// This device's id (read-only, for sim/host harnesses to reconcile a device
    /// with the coordinator-side `DeviceChange`s by id).
    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }
}

#[cfg(test)]
mod smoke {
    //! Host harness proving the lifted loop runs off-hardware (the point of the
    //! lift). Covers the three paths the plan calls out: the real generic
    //! `FrostyUi` over an off-screen `DrawTarget` (a frame renders and a scripted
    //! touch flows through the widget tree), the poll-time `DataErase` → reset, and
    //! the init-time recovery-erase → reset. Firmware/upgrade is punted by
    //! construction (a stub whose upgrade path is never reached).
    use super::*;
    use crate::device_hal::{Clock, TouchEvent, TouchGesture, TouchSource};
    use crate::flash_header::KeyedHash;
    use crate::framed_serial::{ByteIo, SerialPort, WriteError};
    use crate::frosty_ui::FrostyUi;
    use crate::ui::{BusyTask, UiEvent, Workflow};
    use crate::{DownstreamConnectionState, UpstreamConnectionState};
    use alloc::collections::VecDeque;
    use alloc::rc::Rc;
    use bincode::error::{DecodeError, EncodeError};
    use core::cell::{Cell, RefCell};
    use core::marker::PhantomData;
    use embedded_graphics::draw_target::DrawTarget;
    use embedded_graphics::geometry::{OriginDimensions, Point, Size};
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::Pixel;
    use embedded_storage::nor_flash::{self, NorFlash, ReadNorFlash};
    use frostsnap_comms::{Direction, Downstream, Sha256Digest};
    use frostsnap_core::device::DeviceSecretDerivation;

    const SECTORS: usize = 64;

    // --- storage: RefCell-backed RAM NorFlash, owned outside the loop ---

    #[derive(Debug)]
    struct FakeFlash(Box<[u8; 4096 * SECTORS]>);
    impl FakeFlash {
        fn new() -> Self {
            Self(Box::new([0xff; 4096 * SECTORS]))
        }
    }
    impl nor_flash::ErrorType for FakeFlash {
        type Error = core::convert::Infallible;
    }
    impl ReadNorFlash for FakeFlash {
        const READ_SIZE: usize = 1;
        fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
            bytes.copy_from_slice(&self.0[offset as usize..offset as usize + bytes.len()]);
            Ok(())
        }
        fn capacity(&self) -> usize {
            4096 * SECTORS
        }
    }
    impl NorFlash for FakeFlash {
        const WRITE_SIZE: usize = 4;
        const ERASE_SIZE: usize = 4096;
        fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
            self.0[from as usize..to as usize].fill(0xff);
            Ok(())
        }
        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
            self.0[offset as usize..offset as usize + bytes.len()].copy_from_slice(bytes);
            Ok(())
        }
    }

    // --- HAL parts ---

    struct FakeRng(u64);
    impl rand_core::RngCore for FakeRng {
        fn next_u32(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            (self.0 >> 32) as u32
        }
        fn next_u64(&mut self) -> u64 {
            ((self.next_u32() as u64) << 32) | self.next_u32() as u64
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            for chunk in dest.chunks_mut(4) {
                let n = self.next_u32().to_le_bytes();
                chunk.copy_from_slice(&n[..chunk.len()]);
            }
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }
    impl rand_core::CryptoRng for FakeRng {}

    struct FakeSecrets;
    impl KeyedHash for FakeSecrets {
        fn keyed_hash(&mut self, _domain: &str, _input: &[u8]) -> [u8; 32] {
            [1u8; 32]
        }
    }
    impl DeviceSecretDerivation for FakeSecrets {
        fn get_share_encryption_key(
            &mut self,
            _access_structure_ref: frostsnap_core::AccessStructureRef,
            _party_index: frostsnap_core::schnorr_fun::frost::ShareIndex,
            _coord_key: frostsnap_core::CoordShareDecryptionContrib,
        ) -> frostsnap_core::SymmetricKey {
            frostsnap_core::SymmetricKey([0u8; 32])
        }
        fn derive_nonce_seed(
            &mut self,
            _nonce_stream_id: frostsnap_core::nonce_stream::NonceStreamId,
            _index: u32,
            _seed_material: &[u8; 32],
        ) -> [u8; 32] {
            [0u8; 32]
        }
    }

    /// Firmware stub: a seeded digest with an upgrade path that is never reached
    /// (the sim is never offered an upgrade), so the methods are inert.
    struct StubFirmware;
    impl FirmwareServices for StubFirmware {
        fn firmware_digest(&self) -> Sha256Digest {
            Sha256Digest([0u8; 32])
        }
        fn handle<Up, Dn>(
            &mut self,
            _msg: &CoordinatorSendBody,
            _upstream: &mut Up,
            _downstream: Option<&mut Dn>,
            _ui: &mut dyn UserInteraction,
        ) -> FirmwareAction
        where
            Up: SerialPort<Upstream>,
            Dn: SerialPort<Downstream>,
        {
            unreachable!("the sim is never offered a firmware upgrade or challenge")
        }
        fn poll(&mut self, _ui: &mut dyn UserInteraction) -> FirmwareAction {
            FirmwareAction::None
        }
        fn confirm_upgrade(&mut self) {}
        fn cancel(&mut self) {}
    }

    struct FakeByteIo;
    impl ByteIo for FakeByteIo {
        fn read_byte(&mut self) -> Option<u8> {
            None
        }
        fn has_data(&mut self) -> bool {
            false
        }
        fn fill(&mut self) {}
        fn write_bytes(&mut self, _bytes: &[u8]) -> Result<(), WriteError> {
            Ok(())
        }
        fn nb_flush(&mut self) {}
        fn flush(&mut self) {}
        fn set_baud(&mut self, _baud: u32) {}
    }

    /// In-memory serial: no inbound frames. `establishes` makes the magic-bytes
    /// handshake succeed (so the loop leaves `PowerOn`); `resets` records how many
    /// times the loop sent the upstream reset signal.
    struct FakeSerial<D> {
        io: FakeByteIo,
        establishes: bool,
        resets: Rc<Cell<u32>>,
        _direction: PhantomData<D>,
    }
    impl<D> FakeSerial<D> {
        fn new(establishes: bool, resets: Rc<Cell<u32>>) -> Self {
            Self {
                io: FakeByteIo,
                establishes,
                resets,
                _direction: PhantomData,
            }
        }
    }
    impl<D: Direction> SerialPort<D> for FakeSerial<D> {
        fn find_and_remove_magic_bytes(&mut self) -> bool {
            self.establishes
        }
        fn send(
            &mut self,
            _message: <D::Opposite as Direction>::RecvType,
        ) -> Result<(), EncodeError> {
            Ok(())
        }
        fn receive(&mut self) -> Option<Result<frostsnap_comms::ReceiveSerial<D>, DecodeError>>
        where
            frostsnap_comms::ReceiveSerial<D>: bincode::Decode<()>,
        {
            None
        }
        fn write_magic_bytes(&mut self) -> Result<(), EncodeError> {
            Ok(())
        }
        fn write_conch(&mut self) -> Result<(), EncodeError> {
            Ok(())
        }
        fn send_reset_signal(&mut self) -> Result<(), EncodeError> {
            self.resets.set(self.resets.get() + 1);
            Ok(())
        }
        fn flush(&mut self) {}
        fn set_baud(&mut self, _baud: u32) {}
        fn raw(&mut self) -> &mut dyn ByteIo {
            &mut self.io
        }
    }

    struct FakeHal {
        upstream: FakeSerial<Upstream>,
        downstream: FakeSerial<Downstream>,
        rng: FakeRng,
        secrets: FakeSecrets,
        firmware: StubFirmware,
    }
    impl FakeHal {
        fn new(upstream_establishes: bool, resets: Rc<Cell<u32>>) -> Self {
            Self {
                upstream: FakeSerial::new(upstream_establishes, resets),
                downstream: FakeSerial::new(false, Rc::new(Cell::new(0))),
                rng: FakeRng(1),
                secrets: FakeSecrets,
                firmware: StubFirmware,
            }
        }
    }
    impl DeviceHal for FakeHal {
        type Storage = FakeFlash;
        type Upstream = FakeSerial<Upstream>;
        type Downstream = FakeSerial<Downstream>;
        type Rng = FakeRng;
        type Secrets = FakeSecrets;
        type Firmware = StubFirmware;
        fn parts(&mut self) -> HalParts<'_, Self> {
            HalParts {
                upstream: &mut self.upstream,
                downstream: &mut self.downstream,
                rng: &mut self.rng,
                secrets: &mut self.secrets,
                firmware: &mut self.firmware,
            }
        }
        fn keypair_hasher(&mut self) -> &mut dyn KeyedHash {
            &mut self.secrets
        }
    }

    // --- UI fakes ---

    /// An off-screen 240×280 `DrawTarget` that counts the pixels it's asked to
    /// draw, so a test can assert a frame was actually rendered.
    struct OffscreenDisplay {
        drawn: Rc<Cell<u32>>,
    }
    impl OffscreenDisplay {
        fn new(drawn: Rc<Cell<u32>>) -> Self {
            Self { drawn }
        }
    }
    impl OriginDimensions for OffscreenDisplay {
        fn size(&self) -> Size {
            Size::new(240, 280)
        }
    }
    impl DrawTarget for OffscreenDisplay {
        type Color = Rgb565;
        type Error = core::convert::Infallible;
        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            let n = pixels.into_iter().count() as u32;
            self.drawn.set(self.drawn.get() + n);
            Ok(())
        }
    }

    /// A clock that advances 30 ms per read, so `FrostyUi`'s redraw gate fires.
    struct StepClock {
        ms: Cell<u64>,
    }
    impl StepClock {
        fn new() -> Self {
            Self { ms: Cell::new(0) }
        }
    }
    impl Clock for StepClock {
        fn now_ms(&self) -> u64 {
            let next = self.ms.get() + 30;
            self.ms.set(next);
            next
        }
    }

    /// The loop's protocol clock; time-based behaviour isn't under test here.
    struct ZeroClock;
    impl Clock for ZeroClock {
        fn now_ms(&self) -> u64 {
            0
        }
    }

    struct ScriptedTouch {
        events: VecDeque<TouchEvent>,
        pulled: Rc<Cell<u32>>,
    }
    impl ScriptedTouch {
        fn new(events: impl IntoIterator<Item = TouchEvent>, pulled: Rc<Cell<u32>>) -> Self {
            Self {
                events: events.into_iter().collect(),
                pulled,
            }
        }
    }
    impl TouchSource for ScriptedTouch {
        fn next_touch(&mut self) -> Option<TouchEvent> {
            let event = self.events.pop_front();
            if event.is_some() {
                self.pulled.set(self.pulled.get() + 1);
            }
            event
        }
    }

    /// A `UserInteraction` that emits scripted `UiEvent`s, for driving paths whose
    /// real trigger is a hold-to-confirm gesture in the widget tree.
    struct ScriptedUi {
        events: VecDeque<UiEvent>,
    }
    impl ScriptedUi {
        fn new(events: impl IntoIterator<Item = UiEvent>) -> Self {
            Self {
                events: events.into_iter().collect(),
            }
        }
    }
    impl UserInteraction for ScriptedUi {
        fn set_downstream_connection_state(&mut self, _state: DownstreamConnectionState) {}
        fn set_upstream_connection_state(&mut self, _state: UpstreamConnectionState) {}
        fn set_workflow(&mut self, _workflow: Workflow) {}
        fn set_default_workflow(&mut self, _workflow: Workflow) {}
        fn go_to_default(&mut self) {}
        fn set_busy_task(&mut self, _task: BusyTask) {}
        fn clear_busy_task(&mut self) {}
        fn poll(&mut self) -> Option<UiEvent> {
            self.events.pop_front()
        }
        fn force_redraw(&mut self) {}
    }

    #[test]
    fn real_frosty_ui_renders_a_frame_and_takes_a_touch() {
        let flash = RefCell::new(FakeFlash::new());
        let nvs = FlashPartition::new(&flash, 0, SECTORS as u32, "nvs");
        let mut hal = FakeHal::new(false, Rc::new(Cell::new(0)));

        let drawn = Rc::new(Cell::new(0u32));
        let pulled = Rc::new(Cell::new(0u32));
        let touch = ScriptedTouch::new(
            [TouchEvent {
                point: Point::new(120, 140),
                lift_up: false,
                gesture: TouchGesture::None,
            }],
            pulled.clone(),
        );
        let mut ui = FrostyUi::new(
            OffscreenDisplay::new(drawn.clone()),
            StepClock::new(),
            touch,
        );
        let loop_clock = ZeroClock;

        let mut device_loop = match DeviceLoop::new(&mut hal, &mut ui, &loop_clock, nvs) {
            InitOutcome::Ready(device_loop) => device_loop,
            InitOutcome::ResetRequested => panic!("fresh flash should init, not request reset"),
        };

        for _ in 0..3 {
            assert_eq!(device_loop.poll(false), Poll::Continue);
        }

        assert!(
            drawn.get() > 0,
            "the real FrostyUi should have rendered at least one frame off-screen"
        );
        assert!(
            pulled.get() >= 1,
            "the scripted touch should have been delivered through the widget tree"
        );
    }

    #[test]
    fn data_erase_drives_poll_to_reset() {
        let flash = RefCell::new(FakeFlash::new());
        let nvs = FlashPartition::new(&flash, 0, SECTORS as u32, "nvs");
        let resets = Rc::new(Cell::new(0u32));
        // Upstream establishes so the loop leaves PowerOn and actually polls Erase.
        let mut hal = FakeHal::new(true, resets.clone());
        let mut ui = ScriptedUi::new([UiEvent::EraseDataConfirm]);
        let loop_clock = ZeroClock;

        let mut device_loop = match DeviceLoop::new(&mut hal, &mut ui, &loop_clock, nvs) {
            InitOutcome::Ready(device_loop) => device_loop,
            InitOutcome::ResetRequested => panic!("fresh flash should init, not request reset"),
        };

        let mut outcome = Poll::Continue;
        for _ in 0..8 {
            outcome = device_loop.poll(false);
            if outcome == Poll::ResetRequested {
                break;
            }
        }

        assert_eq!(
            outcome,
            Poll::ResetRequested,
            "a confirmed DataErase should drive the loop to a reset"
        );
        assert_eq!(
            resets.get(),
            1,
            "the loop should send exactly one upstream reset signal before resetting"
        );
    }

    #[test]
    fn init_recovery_erase_requests_reset() {
        // Header region (first 2 sectors) blank, but a later sector dirtied: a
        // non-empty NVS with no valid FlashHeader → init runs the recovery erase.
        let mut raw = FakeFlash::new();
        NorFlash::write(&mut raw, 8 * 4096, &[0u8; 4]).unwrap();
        let flash = RefCell::new(raw);
        let nvs = FlashPartition::new(&flash, 0, SECTORS as u32, "nvs");
        let mut hal = FakeHal::new(false, Rc::new(Cell::new(0)));
        let mut ui = ScriptedUi::new([]);
        let loop_clock = ZeroClock;

        match DeviceLoop::new(&mut hal, &mut ui, &loop_clock, nvs) {
            InitOutcome::ResetRequested => { /* expected */ }
            InitOutcome::Ready(_) => {
                panic!("a non-empty NVS without a header must trigger the recovery erase + reset")
            }
        }
    }
}
