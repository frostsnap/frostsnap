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
use frost_backup::verifiable_card::proof::Setup;
use frostsnap_comms::genuine_certificate::DsSignature;
use frostsnap_comms::genuine_check::{self, AttestationSignature, AttestedDevice, DsSignedMessage};
use frostsnap_comms::{
    CommsMisc, CoordinatorSendBody, CoordinatorUpgradeMessage, DeviceName, DeviceSendBody,
    ReceiveSerial, Sha256Digest, Upstream, MAGIC_BYTES_PERIOD,
};
use frostsnap_core::{
    device::{
        card::{CardMutation, CardScreen, HomeLine, ToUserCard},
        card_restore::{
            CardEntries, CardRestoreDecoded, CardRestorePhase, RestoreJob, RestoreOutcome,
            RESTORE_DECODE_MS,
        },
        card_session::{BusyTask as CardTask, CardSession, CardSessionStart},
        restoration::ConsolidatePhase,
        DeviceToUserMessage, FrostSigner, KeyGenPhase3, SignPhase1,
    },
    device_nonces::NonceJobBatch,
    message::{
        self,
        card::{CardHalfEntries, CardRefusal, CoordinatorCard, DeviceCard},
        DeviceSend,
    },
    DeviceId,
};
use frostsnap_core::{Gist, Kind};
use frostsnap_embedded::NonceAbSlot;
use rand_core::RngCore;

use crate::crypto::{AttestError, AttestPoll, EspCrypto, NormalMode};
use crate::frosty_ui::FrostyUi;
use crate::ota::OtaPartitions;
use crate::partitions::EspFlashPartition;
use esp_hal::gpio::Input;
use frostsnap_comms::Downstream;
use rand_chacha::ChaCha20Rng;

type EspSerial<'a, D> = SerialInterface<'a, D>;
type EspMutationLog<'a> = MutationLog<'a, esp_storage::FlashStorage<'static>>;
type EspSigner<'a> = FrostSigner<NonceAbSlot<'a, esp_storage::FlashStorage<'static>>>;

/// Max consecutive magic-bytes frames tolerated after we've replied to the
/// coord but before it ack's. Absorbs the coord's `awaiting_magic` retry loop,
/// which may queue several frames before reading our reply (in-flight noise).
/// At the 100 ms coord cadence this is ~1 s before we give up and re-handshake.
const MAGIC_BYTES_RESET_THRESHOLD: u32 = 9;

/// A card restore (design §3.8), from `RestoreEntry` until its report is left: the words 1-32
/// the user confirmed, kept for "Edit words 1-32", and the decode core asked for. Core holds
/// both halves; this holds only what the firmware needs. The entries, the job's copy of the
/// card and a decoded share are wiped when it's dropped.
struct CardRestore {
    phase: CardRestorePhase,
    entries: Option<CardHalfEntries>,
    decode: Option<CardDecode>,
}

/// The decode of one generation of the halves, from `RestoreDecode` until T_dec.
struct CardDecode {
    generation: u32,
    job: RestoreJob,
    /// When the decode started: the report waits for [`RESTORE_DECODE_MS`] from here.
    started: Instant,
    outcome: Option<RestoreOutcome>,
}

/// Timing of one card job for a first run on the device: wall clock, compute, slices and the
/// longest one, and the heap. It goes to the on-screen log (`debug_log`) only, never upstream
/// (spec L1.9.8).
struct CardJobClock {
    job: Option<&'static str>,
    started: Instant,
    slices: u32,
    paused: u32,
    compute: Duration,
    longest: Duration,
    heap_peak: usize,
}

impl CardJobClock {
    fn new() -> Self {
        Self {
            job: None,
            started: Instant::EPOCH,
            slices: 0,
            paused: 0,
            compute: Duration::ZERO,
            longest: Duration::ZERO,
            heap_peak: 0,
        }
    }

    fn slice(&mut self, job: &'static str, started: Instant) {
        let took = started.elapsed();
        if self.job != Some(job) {
            self.finish("stopped");
            *self = Self {
                job: Some(job),
                started,
                ..Self::new()
            };
        }
        self.slices += 1;
        self.compute += took;
        self.longest = self.longest.max(took);
        self.heap_peak = self.heap_peak.max(esp_alloc::HEAP.used());
    }

    /// A poll whose slice was skipped while the user interacted.
    fn pause(&mut self) {
        if self.job.is_some() {
            self.paused += 1;
        }
    }

    fn finish(&mut self, how: &str) {
        let Some(job) = self.job.take() else {
            return;
        };
        card_log(|| {
            format!(
                "card {job} {how}: {}ms wall, {}ms compute, {} slices (longest {}ms), {} paused, heap {} peak {}{}",
                self.started.elapsed().as_millis(),
                self.compute.as_millis(),
                self.slices,
                self.longest.as_millis(),
                self.paused,
                esp_alloc::HEAP.used(),
                self.heap_peak,
                heap_max(),
            )
        });
    }
}

/// A card session about to be opened by this poll's outbox.
fn opens_card_session(send: &DeviceSend) -> bool {
    matches!(send, DeviceSend::ToUser(message)
        if matches!(&**message, DeviceToUserMessage::Card(card)
            if matches!(**card, ToUserCard::OpenSession(_))))
}

/// A card frame to the phone that answers it (an opening's part, the status that carries the
/// seal's facts, or words 33-64), which this poll's drain hands to the upstream writer: the
/// poll that queues it computes no job slice.
fn is_card_frame(send: &DeviceSend) -> bool {
    matches!(send, DeviceSend::ToCoordinator(message)
    if matches!(
        **message,
        message::DeviceToCoordinatorMessage::Card(
            DeviceCard::OpeningPart(_) | DeviceCard::Status(_) | DeviceCard::PhoneHalf(_)
        )
    ))
}

/// The card screen a send shows, if it shows one.
fn card_screen(send: &DeviceSend) -> Option<&CardScreen> {
    match send {
        DeviceSend::ToUser(message) => match &**message {
            DeviceToUserMessage::Card(card) => match &**card {
                ToUserCard::Screen(screen) => Some(screen),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// The card screens that replace any screen (design §2.1 item 9).
fn is_card_alarm(screen: &CardScreen) -> bool {
    matches!(
        screen,
        CardScreen::PhoneRoundFailed
            | CardScreen::Stopped
            | CardScreen::DeviceError
            | CardScreen::Refused { .. }
    )
}

fn card_job_name(task: CardTask) -> &'static str {
    match task {
        CardTask::Seal => "seal",
        CardTask::Proofs => "proofs",
        CardTask::Opening => "opening",
        CardTask::Card => "encode",
    }
}

/// The heap's high-water mark, with the `card_heap_stats` feature.
#[cfg(feature = "card_heap_stats")]
fn heap_max() -> alloc::string::String {
    format!(" max {}", esp_alloc::HEAP.stats().max_usage)
}

#[cfg(not(feature = "card_heap_stats"))]
fn heap_max() -> alloc::string::String {
    alloc::string::String::new()
}

/// A line for the on-screen log, built only when there is one, and for the `card-timing`
/// dump.
fn card_log(line: impl FnOnce() -> alloc::string::String) {
    #[cfg(any(feature = "debug_log", feature = "card-timing"))]
    {
        let line = line();
        #[cfg(feature = "card-timing")]
        card_timing::keep(&line);
        #[cfg(feature = "debug_log")]
        frostsnap_widgets::debug::log(line);
    }
    #[cfg(not(any(feature = "debug_log", feature = "card-timing")))]
    let _ = line;
}

struct DeviceLoop<'a> {
    // Borrowed from Resources
    rng: &'a mut ChaCha20Rng,
    crypto: &'a mut EspCrypto<'a>,
    ota_partitions: &'a mut OtaPartitions<'a>,
    ui: &'a mut FrostyUi<'a>,
    upstream_serial: &'a mut EspSerial<'a, Upstream>,
    downstream_serial: &'a mut EspSerial<'a, Downstream>,
    downstream_detect: &'a mut Input<'a>,

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
    ui_events: VecDeque<UiEvent>,
    nonce_task_batch: Option<NonceJobBatch>,
    inbox: Vec<CoordinatorSendBody>,
    next_write_magic_bytes_downstream: Instant,
    magic_bytes_timeout_counter: u32,
    upgrade: Option<ota::FirmwareUpgradeMode<'a>>,
    erase_state: Option<erase::Erase>,
    pending_device_name: Option<DeviceName>,

    // Card flows: at most one session or restore at a time
    card_setup: &'static Setup,
    card_session: Option<CardSession<'static>>,
    /// A card restore, from `RestoreEntry` until its report is left.
    card_restore: Option<CardRestore>,
    card_clock: CardJobClock,
}

impl<'a> DeviceLoop<'a> {
    /// Box in here rather than the caller so LLVM can construct directly into the heap
    /// allocation and avoid putting the struct on the caller's stack.
    #[inline(never)]
    fn new(resources: &'a mut Resources<'a>) -> Box<Self> {
        let Resources {
            ref mut rng,
            ref mut crypto,
            ref mut nvs,
            ota: ref mut ota_partitions,
            ref mut ui,
            ref mut upstream_serial,
            ref mut downstream_serial,
            ref mut downstream_detect,
        } = resources;
        let ui = &mut **ui;

        let full_nvs = *nvs;

        let header_sectors = nvs.split_off_front(2);
        let header_flash = crate::flash::FlashHeader::new(header_sectors);
        let header = match header_flash.read_header() {
            Some(h) => h,
            None => {
                if !nvs.is_empty().expect("checking NVS is empty") {
                    let mut erase_op = erase::Erase::new(&full_nvs);
                    while !matches!(erase_op.poll(&full_nvs, ui), erase::ErasePoll::Reset) {}
                    esp_hal::system::software_reset();
                }
                header_flash.init(rng)
            }
        };
        let device_keypair = header.device_keypair(&mut crypto.force_normal_mode().fixed_entropy());

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
        let active_firmware_digest = crypto
            .force_normal_mode()
            .digest(&active_partition, Some(firmware_size));

        let device_id = signer.device_id();

        ui.set_default_workflow(default_workflow(name.as_ref(), &signer));
        ui.go_to_default();

        let upstream_connection = UpstreamConnection::new(device_id);
        ui.set_upstream_connection_state(upstream_connection.state);
        ui.clear_busy_task();

        Box::new(Self {
            rng,
            crypto,
            ota_partitions,
            ui,
            upstream_serial,
            downstream_serial,
            downstream_detect,
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
            ui_events: VecDeque::new(),
            nonce_task_batch: None,
            inbox: vec![],
            next_write_magic_bytes_downstream: Instant::EPOCH,
            magic_bytes_timeout_counter: 0,
            upgrade: None,
            erase_state: None,
            pending_device_name: None,
            card_setup: Box::leak(Box::new(Setup::new())),
            card_session: None,
            card_restore: None,
            card_clock: CardJobClock::new(),
        })
    }

    fn update_default_workflow(&mut self) {
        self.ui
            .set_default_workflow(default_workflow(self.name.as_ref(), &self.signer));
    }

    /// A card session or a restore is live, or core holds a restore's half in RAM: nothing
    /// goes upstream that would show the phone the timing of what the user does (spec
    /// L1.9.8), from `EnterCard` until the restore ends (design §3.8).
    fn card_flow_active(&self) -> bool {
        self.card_session.is_some()
            || self.card_restore.is_some()
            || self.signer.card_restore_active()
    }

    /// A debug line upstream, unless a card flow is live.
    fn send_debug(&mut self, message: impl alloc::string::ToString) {
        if !self.card_flow_active() {
            self.upstream_connection.send_debug(message);
        }
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
            self.next_write_magic_bytes_downstream = Instant::EPOCH;
            self.update_default_workflow();
            self.ui.go_to_default();
            self.upgrade = None;
            self.pending_device_name = None;
            self.outbox.clear();
            self.nonce_task_batch = None;
            self.crypto.force_normal_mode();
            self.inbox.clear();
            self.ui_events.clear();
            // The app restarted: the card session ends and the device is home, as on an
            // unplug. The card record keeps the progress, and the next `Begin` resumes it.
            self.end_card_flow();
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
                let now = Instant::now();
                if now > self.next_write_magic_bytes_downstream {
                    self.next_write_magic_bytes_downstream =
                        now + Duration::from_millis(MAGIC_BYTES_PERIOD);
                    self.downstream_serial
                        .write_magic_bytes()
                        .expect("couldn't write magic bytes downstream");
                }
                if self.downstream_serial.find_and_remove_magic_bytes() {
                    self.downstream_connection_state = DownstreamConnectionState::Established;
                    self.ui
                        .set_downstream_connection_state(self.downstream_connection_state);
                    self.send_debug("Device read magic bytes from another device!");
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
                                self.send_debug("downstream device sent unexpected magic bytes");
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
                        self.send_debug(format!("Failed to decode on downstream port: {e}"));
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
                                                    let outcome = upgrade.enter_upgrade_mode(
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
                                                        &mut self.crypto.force_normal_mode(),
                                                    );
                                                    match outcome {
                                                        ota::UpgradeOutcome::Committed => {
                                                            reset(self.upstream_serial)
                                                        }
                                                        // Any refusal parks here until power
                                                        // cycle. The active slot is unchanged
                                                        // and the screen says why and how to
                                                        // recover; resuming the loop instead
                                                        // would leave the coordinator and the
                                                        // chain in a half-finished upgrade
                                                        // conversation.
                                                        ota::UpgradeOutcome::Rejected => loop {
                                                            self.ui.poll();
                                                        },
                                                    }
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

        if let Some(message) = self.upstream_connection.dequeue_message() {
            // the hand-off to the upstream writer, one frame per poll (addendum §2.10 (b))
            self.upstream_serial
                .send(message)
                .expect("failed to send message upstream");
        }

        self.ui_events.extend(self.ui.poll());

        if let AttestPoll::Done(outcome) = self.crypto.poll() {
            self.send_factory_attestation(outcome);
        }
        // Everything below may use the HMAC (`normal_mode`), so none of it runs while a factory
        // attestation is in flight.
        if self.crypto.try_normal_mode().is_none() {
            return;
        }

        let genuine_check = self.process_inbox();

        // Between the inbox and the flush, so what a job event stages is on flash before the
        // outbox sends anything that depends on it
        self.poll_card();

        self.flush_staged();

        // Poll nonce job batch
        if let Some(batch) = self.nonce_task_batch.as_mut() {
            log_and_redraw!(self.ui, "nonce batch start");
            if batch.do_work(&mut normal_mode(self.crypto).share_encryption()) {
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
        self.drain_outbox();

        // Handle UI events
        while let Some(ui_event) = self.ui_events.pop_front() {
            self.handle_ui_event(ui_event);
        }

        #[cfg(feature = "card-timing")]
        if !self.card_flow_active() {
            card_timing::dump(&mut self.upstream_connection);
        }

        // Answered at the end of the iteration: a factory attestation starts the DS, which must
        // not run while the HMAC is still to be used above.
        if let Some(message) = genuine_check {
            self.genuine_check(&message);
        }
    }

    /// Acts on what the user did. Out of line to keep `poll`'s frame within the stack
    /// budget.
    #[inline(never)]
    fn handle_ui_event(&mut self, ui_event: UiEvent) {
        match ui_event {
            UiEvent::KeyGenConfirm { phase } => self.keygen_confirm(phase),
            UiEvent::SigningConfirm { phase } => self.signing_confirm(phase),
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
            UiEvent::ConsolidateConfirm { phase } => {
                self.finish_consolidation(*phase);
                self.ui.go_to_default();
            }
            card_event @ (UiEvent::Card(_)
            | UiEvent::CardExit
            | UiEvent::CardRestoreEntered { .. }
            | UiEvent::CardRestoreEdit
            | UiEvent::CardRestoreConfirm(_)) => self.card_ui_event(card_event),
        }
    }

    /// Out of line, as the keygen ack is heavy on the stack and would otherwise sit in
    /// `handle_ui_event`'s frame under every UI event, the card ones included.
    #[inline(never)]
    fn keygen_confirm(&mut self, phase: Box<KeyGenPhase3>) {
        self.outbox.extend(
            self.signer
                .keygen_ack(
                    *phase,
                    &mut normal_mode(self.crypto).share_encryption(),
                    self.rng,
                )
                .expect("state changed while confirming keygen"),
        );
        self.ui.clear_busy_task();
    }

    /// Out of line, as the sign ack is heavy on the stack and would otherwise sit in
    /// `handle_ui_event`'s frame under every UI event, the card ones included.
    #[inline(never)]
    fn signing_confirm(&mut self, phase: Box<SignPhase1>) {
        self.ui.set_busy_task(ui::BusyTask::Signing);
        self.outbox.extend(
            self.signer
                .sign_ack(*phase, &mut normal_mode(self.crypto).share_encryption())
                .expect("state changed while acking sign"),
        );
    }

    /// Hands each message from the coordinator to core, or acts on it. Out of line to keep
    /// `poll`'s frame within the stack budget. Returns a genuine check, which `poll` answers at
    /// the end of the iteration.
    #[inline(never)]
    fn process_inbox(&mut self) -> Option<genuine_check::CoordinatorMessage> {
        // Process inbox messages
        let mut genuine_check = None;
        let mut inbox = core::mem::take(&mut self.inbox);
        for message_body in inbox.drain(..) {
            match &message_body {
                CoordinatorSendBody::Cancel => {
                    self.signer.clear_tmp_data();
                    self.ui.go_to_default();
                    self.ui_events.clear();
                    self.pending_device_name = None;
                    self.upgrade = None;
                    // the card record stays, so a Cancel only interrupts a card flow
                    self.end_card_flow();
                    // a card screen queued before the Cancel would otherwise come up after it,
                    // with no session; not in end_card_flow, which runs mid-drain on an
                    // OpenSession
                    self.outbox.retain(|send| {
                        !matches!(send, DeviceSend::ToUser(message)
                            if matches!(**message, DeviceToUserMessage::Card(_)))
                    });
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
                CoordinatorSendBody::Core(message::CoordinatorToDeviceMessage::Card(
                    card_message,
                )) => {
                    log_and_redraw!(self.ui, "process: {}", card_message.kind());
                    self.recv_card_message(card_message.clone());
                }
                CoordinatorSendBody::Core(core_message) => self.recv_core_message(core_message),
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
                CoordinatorSendBody::_LegacyChallenge(_) => {}
                CoordinatorSendBody::GenuineCheck(message) => {
                    genuine_check = Some(message.clone());
                }
            }
        }
        self.inbox = inbox;
        genuine_check
    }

    /// Hands a core message to the signer. Out of line, as are the card messages, so
    /// `process_inbox`'s frame stays small under each (the card chains are the deepest on the
    /// stack).
    #[inline(never)]
    fn recv_core_message(&mut self, core_message: &message::CoordinatorToDeviceMessage) {
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

    #[inline(never)]
    fn genuine_check(&mut self, message: &genuine_check::CoordinatorMessage) {
        let Some(attested) = self.attested_device() else {
            return;
        };
        let attested_digest = attested.digest();
        match message.device_action(attested_digest) {
            genuine_check::DeviceAction::FactoryAttest(challenge) => {
                let signed = DsSignedMessage::GenuineAttestationV0 {
                    challenge,
                    attested,
                };
                if let Err(e) = self.crypto.start_attestation(&signed.digest()) {
                    log!("factory attestation failed to start: {e:?}");
                }
            }
            genuine_check::DeviceAction::IdentityAttest(challenge) => {
                let schnorr = frostsnap_core::schnorr_fun::new_with_deterministic_nonces::<
                    frostsnap_core::sha2::Sha256,
                >();
                let signature = genuine_check::sign_identity_attestation(
                    &schnorr,
                    self.signer.keypair(),
                    challenge,
                    attested_digest,
                );
                self.upstream_connection
                    .send_to_coordinator([DeviceSendBody::GenuineCheck(
                        genuine_check::DeviceMessage::IdentityAttestation { signature },
                    )]);
            }
        }
    }

    fn send_factory_attestation(&mut self, outcome: Result<Box<DsSignature>, AttestError>) {
        let signature = match outcome {
            Ok(signature) => signature,
            Err(e) => {
                log!("factory attestation failed: {e:?}");
                return;
            }
        };
        let Some(attested) = self.attested_device() else {
            return;
        };
        self.upstream_connection
            .send_to_coordinator([DeviceSendBody::GenuineCheck(
                genuine_check::DeviceMessage::FactoryAttestation {
                    attested: Box::new(attested),
                    signature: Box::new(AttestationSignature::Rsa3072(*signature)),
                },
            )]);
    }

    fn attested_device(&self) -> Option<AttestedDevice> {
        Some(AttestedDevice::V0 {
            device_id: self.device_id,
            certificate: self.crypto.certificate()?,
        })
    }

    /// Sends or shows what core output. Out of line to keep `poll`'s frame within the stack
    /// budget.
    #[inline(never)]
    fn drain_outbox(&mut self) {
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
                                        .decrypt_to_backup(
                                            &mut normal_mode(self.crypto).share_encryption(),
                                        )
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
                                        .decrypt_to_backup(
                                            &mut normal_mode(self.crypto).share_encryption(),
                                        )
                                        .expect("state changed while checking backup");
                                    let rand_seed = self.rng.next_u32();
                                    self.ui.set_workflow(ui::Workflow::CheckBackup {
                                        key_name: key_name.to_string(),
                                        backup,
                                        access_structure_ref,
                                        rand_seed,
                                    });
                                }
                                ConsolidateBackup(phase) => match phase.threshold_warning {
                                    // A card share whose threshold isn't the wallet's, or
                                    // doesn't read: the user holds through the warning first,
                                    // and it never refuses (build plan A11)
                                    Some(warning) => self.ui.set_workflow(ui::Workflow::prompt(
                                        ui::Prompt::ConsolidateThreshold {
                                            phase: Box::new(phase),
                                            warning,
                                        },
                                    )),
                                    // Auto-confirm: the user can't meaningfully verify this.
                                    // Another wallet's threshold hold stays up, or its
                                    // coordinator would wait on it until a replug.
                                    None => {
                                        self.finish_consolidation(phase);
                                        if !self.ui.consolidate_hold_pending() {
                                            self.ui.go_to_default();
                                        }
                                    }
                                },
                            }
                        }
                        DeviceToUserMessage::NonceJobs(batch) => {
                            self.nonce_task_batch = Some(batch);
                        }
                        DeviceToUserMessage::Card(card) => self.card_to_user(card),
                    };
                }
            }
        }
    }

    /// Saves a consolidated share and tells the coordinator. The caller decides whether to
    /// leave the screen that's up. Out of line, as the share's encryption is heavy on the
    /// stack and runs under the drain and under a UI event.
    #[inline(never)]
    fn finish_consolidation(&mut self, phase: ConsolidatePhase) {
        // Collected here, as the sends would otherwise hold the HMAC's borrow over the flush
        let sends: Vec<DeviceSend> = self
            .signer
            .finish_consolidation(
                &mut normal_mode(self.crypto).share_encryption(),
                phase,
                self.rng,
            )
            .into_iter()
            .collect();
        // FinishedConsolidation is queued only once the share is on flash
        self.flush_staged();
        self.outbox.extend(sends);

        self.save_pending_device_name();
        self.update_default_workflow();
    }

    /// Drops the card session or the restore and its decode, wiping what they hold. Out of
    /// line, as the wipes are heavy on the stack.
    #[inline(never)]
    fn end_card_flow(&mut self) {
        self.card_session = None;
        self.card_restore = None;
    }

    /// Writes the staged core mutations to flash.
    #[inline(never)]
    fn flush_staged(&mut self) {
        let staged_mutations = self.signer.staged_mutations();
        if staged_mutations.is_empty() {
            return;
        }
        // A card mutation follows what the user did (words written, a quiz passed, a pick), so
        // a debug frame about it would show the phone its timing (spec L1.9.8). A card
        // mutation can outlive its session by a poll (a Cancel before the flush), so the batch
        // is checked too.
        let card_mutation = staged_mutations
            .iter()
            .any(|mutation| matches!(mutation, frostsnap_core::device::Mutation::Card(_)));
        // Only a new record, a verdict or a failed device round changes the home line (a
        // terminal record takes no more mutations), so the steps of writing and the quizzes
        // don't rebuild it
        let home_line_changed = staged_mutations.iter().any(|mutation| {
            matches!(
                mutation,
                frostsnap_core::device::Mutation::Card(
                    CardMutation::Origin { .. }
                        | CardMutation::Verdict { .. }
                        | CardMutation::DeviceRoundResult { .. }
                )
            )
        });
        let start = Instant::now();
        self.mutation_log
            .append(staged_mutations.drain(..).map(Mutation::Core))
            .expect("writing core mutations failed");
        if card_mutation {
            if home_line_changed {
                // design §7.5 D21
                self.update_default_workflow();
            }
        } else {
            self.send_debug(format!(
                "core mutations took {}ms",
                start.elapsed().as_millis()
            ));
        }
    }

    #[inline(never)]
    fn recv_card_message(&mut self, message: CoordinatorCard) {
        let sends =
            if matches!(message, CoordinatorCard::Begin(_)) && !self.upstream_serial.is_jtag() {
                // card frames stall a daisy chain (design §9 R6)
                self.signer.card_refusal(CardRefusal::NotDirectUsb)
            } else {
                self.signer
                    .recv_card_message(message, self.card_session.as_mut())
            };
        self.outbox.extend(sends);
    }

    // The box is kept, not moved out of, so the words of a screen are wiped where core put
    // them
    #[allow(clippy::boxed_local)]
    #[inline(never)]
    fn card_to_user(&mut self, message: Box<ToUserCard>) {
        if let ToUserCard::Screen(screen) = &*message {
            self.ui.set_workflow(ui::Workflow::Card(screen.clone()));
            return;
        }
        match *message {
            ToUserCard::OpenSession(start) => self.card_open_session(start),
            ToUserCard::Screen(_) => {}
            ToUserCard::CloseSession => self.end_card_flow(),
            ToUserCard::RestoreEntry(phase) => self.card_restore_entry(phase),
            ToUserCard::RestoreDecode {
                phase,
                generation,
                entries,
            } => self.card_restore_decode(phase, generation, &entries),
            ToUserCard::RestoreReport(report) => self
                .ui
                .set_workflow(ui::Workflow::CardRestoreReport(report)),
        }
    }

    /// Opens a card session. Out of line, as is the restore decode: their frames (the session's
    /// secrets, the restore job) must not stay live under `set_workflow` on every card screen,
    /// the deepest path on the stack.
    #[inline(never)]
    fn card_open_session(&mut self, start: Box<CardSessionStart>) {
        // first drop the old one, so two seals and proof buffers never coexist
        self.end_card_flow();
        self.card_clock.finish("dropped");
        let started = Instant::now();
        match start.open(
            &mut normal_mode(self.crypto).share_encryption(),
            self.card_setup,
            &mut *self.rng,
        ) {
            Ok(mut session) => {
                card_log(|| {
                    format!(
                        "card session open: {}ms, heap {}",
                        started.elapsed().as_millis(),
                        esp_alloc::HEAP.used()
                    )
                });
                self.outbox
                    .extend(self.signer.card_session_opened(&mut session));
                self.card_session = Some(session);
            }
            Err(_) => self
                .outbox
                .extend(self.signer.card_refusal(CardRefusal::DeviceError)),
        }
    }

    /// Both halves are in: the decode of `generation` starts, and the report waits for T_dec
    /// from now (design §3.8). A decode already running is dropped: core discards its result
    /// anyway, as its halves have changed.
    #[inline(never)]
    fn card_restore_decode(
        &mut self,
        phase: CardRestorePhase,
        generation: u32,
        entries: &CardEntries,
    ) {
        let job = RestoreJob::new(
            entries,
            phase.target.as_ref(),
            phase.recorded_format,
            phase.share_indices.as_deref(),
        );
        self.card_clock.finish("dropped");
        let decode = CardDecode {
            generation,
            job,
            started: Instant::now(),
            outcome: None,
        };
        match self.card_restore.as_mut() {
            Some(restore) => restore.decode = Some(decode),
            None => {
                self.card_restore = Some(CardRestore {
                    phase,
                    entries: None,
                    decode: Some(decode),
                })
            }
        }
    }

    /// The phone asked for a card restore (`EnterCard`): refused off direct USB, as `Begin` is;
    /// otherwise the entry of words 1-32 opens from word 1, ending any card session. After a
    /// reconnect the device has booted fresh, so its entry starts again too (design §5.3 E6).
    #[inline(never)]
    fn card_restore_entry(&mut self, phase: CardRestorePhase) {
        if !self.upstream_serial.is_jtag() {
            // card frames stall a daisy chain (design §9 R6)
            self.outbox
                .extend(self.signer.card_refusal(CardRefusal::NotDirectUsb));
            return;
        }
        self.end_card_flow();
        self.card_clock.finish("dropped");
        self.card_restore = Some(CardRestore {
            phase: phase.clone(),
            entries: None,
            decode: None,
        });
        self.ui.set_workflow(ui::Workflow::CardRestoreEntry {
            phase: Box::new(phase),
            review: None,
        });
    }

    #[inline(never)]
    fn card_ui_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Card(action) => {
                let had_session = self.card_session.is_some();
                let sends = self
                    .signer
                    .card_user_action(self.card_session.as_mut(), action);
                // a verdict screen needs no session, so its next page still comes; any other
                // screen's flow was interrupted (a Cancel), so leave rather than freeze
                if sends.is_empty() && !had_session {
                    self.ui.go_to_default();
                }
                self.outbox.extend(sends);
            }
            UiEvent::CardRestoreEntered { phase, entries } => {
                let Some(restore) = self.card_restore.as_mut() else {
                    // the restore is gone (a Cancel), so leave rather than freeze
                    self.ui.go_to_default();
                    return;
                };
                restore.entries = Some(entries.clone());
                self.outbox
                    .extend(self.signer.card_restore_device_half(&phase, entries.0));
            }
            UiEvent::CardRestoreEdit => {
                let Some(restore) = self.card_restore.as_mut() else {
                    self.ui.go_to_default();
                    return;
                };
                // words 1-32 are out until they're confirmed again
                restore.decode = None;
                let workflow = ui::Workflow::CardRestoreEntry {
                    phase: Box::new(restore.phase.clone()),
                    review: restore.entries.clone(),
                };
                self.outbox
                    .extend(self.signer.card_restore_edit_device_half());
                self.ui.set_workflow(workflow);
            }
            UiEvent::CardExit => {
                self.end_card_flow();
                self.signer.card_restore_left();
                self.update_default_workflow();
                self.ui.go_to_default();
            }
            UiEvent::CardRestoreConfirm(decoded) => {
                self.end_card_flow();
                // cloned, not moved out of the box, so the share is wiped where it lives
                self.outbox.extend(
                    self.signer
                        .card_restore_confirmed(CardRestoreDecoded::clone(&decoded)),
                );
                self.ui.go_to_default();
            }
            _ => {}
        }
    }

    /// One slice of the card job: the session's, or the restore's decode.
    #[inline(never)]
    fn poll_card(&mut self) {
        if let Some(session) = self.card_session.as_mut() {
            // a nonce slice or a session being opened already fills this poll, and a card
            // frame about to go out keeps its poll free of job slices, so the time from a tap
            // to its frame doesn't depend on the job (addendum §2.10 (a))
            let busy = self.nonce_task_batch.is_some()
                || self
                    .outbox
                    .iter()
                    .any(|send| opens_card_session(send) || is_card_frame(send));
            if busy || self.ui.is_interacting() {
                self.card_clock.pause();
                return;
            }
            let Some(task) = session.progress().map(|progress| progress.task) else {
                return;
            };
            let started = Instant::now();
            let event = session.step(&mut *self.rng);
            self.card_clock.slice(card_job_name(task), started);
            if let Some(event) = event {
                self.card_clock.finish("done");
                let keep_screen = !self.ui.may_show_card_job_screen();
                // nothing is drawn over a screen that isn't the session's, or at home
                // (addendum U5)
                let may_draw = !keep_screen && !self.ui.at_standby();
                let sends = self.signer.card_job_event(session, event, may_draw);
                // a job's next screen never replaces the user's screen; an alarm does
                self.outbox.extend(sends.into_iter().filter(|send| {
                    !(keep_screen && card_screen(send).is_some_and(|screen| !is_card_alarm(screen)))
                }));
            }
            // PROVING's bar runs over the seal's recompute and the proofs (owner B2)
            if let Some(progress) = session.proving_progress() {
                self.ui.set_card_progress(progress);
            } else if let Some(progress) = session.progress().filter(|progress| {
                matches!(
                    progress.task,
                    CardTask::Seal | CardTask::Card | CardTask::Proofs
                )
            }) {
                self.ui
                    .set_card_progress(progress.done as f32 / progress.total.max(1) as f32);
            }
        } else if self
            .card_restore
            .as_ref()
            .is_some_and(|restore| restore.decode.is_some())
        {
            self.poll_card_restore();
        }
    }

    /// One slice of a restore's decode, and its report at T_dec. Out of line, so the decode's
    /// frame isn't under every session slice.
    #[inline(never)]
    fn poll_card_restore(&mut self) {
        let Some(decode) = self
            .card_restore
            .as_mut()
            .and_then(|restore| restore.decode.as_mut())
        else {
            return;
        };
        // The Checking screen takes no touches, so the decode pauses only for the fade into it,
        // and while a frame to the phone waits in the outbox: as for a session's slices
        // (addendum §2.10 (a)), the poll that queues a reply computes no slice (review S1). The
        // search is a fixed count of slices, not a time, so a phone that keeps a reply queued
        // only delays the outcome and never shortens the search (review B15 secrecy S-2)
        if self.ui.is_transitioning()
            || self
                .outbox
                .iter()
                .any(|send| matches!(send, DeviceSend::ToCoordinator(_)))
        {
            self.card_clock.pause();
            return;
        }
        let elapsed = decode.started.elapsed().as_millis();
        let started = Instant::now();
        // every slice does the same work, whether the search is still going, is over, or found
        // the card (owner R2-1), the search always runs its fixed count of slices, and the
        // outcome comes out at T_dec at the earliest, so neither where slices end nor when the
        // decode is done shows the phone anything
        if let Some(outcome) = decode.job.step(elapsed) {
            self.card_clock.slice("restore", started);
            self.card_clock.finish("done");
            decode.outcome = Some(outcome);
        } else {
            self.card_clock.slice("restore", started);
        }
        self.ui.set_card_progress(decode.job.progress());
        if elapsed < RESTORE_DECODE_MS || decode.outcome.is_none() {
            return;
        }
        let Some(CardDecode {
            generation,
            outcome: Some(outcome),
            ..
        }) = self
            .card_restore
            .as_mut()
            .and_then(|restore| restore.decode.take())
        else {
            return;
        };
        // core sends the phone only whether a share was restored, or decodes again if a
        // half changed meanwhile
        self.outbox
            .extend(self.signer.card_restore_decoded(generation, outcome));
    }
}

/// The home screen: the key, with its card line (design §7.5 D21), or the welcome. On a device
/// holding several shares, a share whose card has a line is shown first, so the line is never
/// hidden behind another key (R20).
/// The crypto peripherals for a message or an event. [`DeviceLoop::poll`] handles those only
/// while no factory attestation is in flight, and starts one only after them.
fn normal_mode<'c, 'd>(crypto: &'c mut EspCrypto<'d>) -> NormalMode<'c, 'd> {
    crypto
        .try_normal_mode()
        .expect("no factory attestation is in flight while messages and events are handled")
}

fn default_workflow(name: Option<&DeviceName>, signer: &EspSigner<'_>) -> ui::Workflow {
    // The most severe card line first, so a failed backup's alarm (design §7.5 D21) never
    // hides behind another key's "not checked"; the first share when none has a line.
    let severity = |line: Option<HomeLine>| match line {
        Some(HomeLine::Failed) => 0,
        Some(HomeLine::NotChecked) => 1,
        None => 2,
    };
    let shown = signer
        .held_shares()
        .map(|held_share| {
            let card_line = signer.card_home_line(held_share.share_image);
            (held_share, card_line)
        })
        .min_by_key(|(_, card_line)| severity(*card_line));
    match (name, shown) {
        (Some(device_name), Some((held_share, card_line))) => ui::Workflow::Standby {
            device_name: device_name.clone(),
            card_line,
            held_share,
        },
        _ => ui::Workflow::None,
    }
}

/// Main event loop for the device
pub fn run<'a>(resources: &'a mut Resources<'a>) -> ! {
    let mut device_loop = DeviceLoop::new(resources);
    #[cfg(feature = "card-timing")]
    card_timing::boot_bench();
    loop {
        device_loop.poll();
    }
}

fn reset(upstream_serial: &mut SerialInterface<Upstream>) -> ! {
    let _ = upstream_serial.send_reset_signal();
    esp_hal::system::software_reset()
}

/// The `card-timing` bench build (design §7.3, U3): the owner flashes it and runs a card
/// through, and the job timings come out upstream as debug lines once no card flow is live.
///
/// Every line [`card_log`] writes is kept: the session open, and for each job (the encode,
/// the seal, the proofs, each opening, the restore decode) its wall clock, compute, slices,
/// longest slice and heap peak (`card_heap_stats` adds the allocator's own high-water mark).
/// At boot it also times one encode slice by slice and the two worst restore repairs the
/// design names, 2 wrong words with no erasure and with 1, on a fixed share, the cases U1's
/// op counts pin.
///
/// 🔐 The lines carry job timings, which follow the user's touches (a job pauses for them),
/// so they wait until the card flow is over. Never ship it, and use it only with a disposable
/// wallet.
#[cfg(feature = "card-timing")]
mod card_timing {
    use crate::{Instant, UpstreamConnection, UpstreamConnectionState};
    use alloc::{boxed::Box, format, string::String, vec::Vec};
    use core::cell::RefCell;
    use critical_section::Mutex;
    use frost_backup::verifiable_card::{codec, layout::WORDS};
    use frostsnap_core::{
        device::card_restore::{CardEntries, RestoreJob, RestoreOutcome},
        schnorr_fun::{
            frost::{SecretShare, SharedKey},
            fun::{
                marker::{Public, Secret, Zero},
                poly, Scalar,
            },
        },
    };

    /// The most lines kept, so a long run can't fill the heap.
    const CAPACITY: usize = 96;

    static LINES: Mutex<RefCell<Vec<String>>> = Mutex::new(RefCell::new(Vec::new()));

    /// Keeps a line for the dump.
    pub fn keep(line: &str) {
        critical_section::with(|cs| {
            let mut lines = LINES.borrow_ref_mut(cs);
            if lines.len() < CAPACITY {
                lines.push(format!("card-timing {line}"));
            }
        });
    }

    /// Every line kept so far, upstream, then forgotten. Called only while no card flow is
    /// live; waits for the coordinator to be there.
    pub fn dump(upstream: &mut UpstreamConnection) {
        if upstream.get_state() != UpstreamConnectionState::EstablishedAndCoordAck {
            return;
        }
        let lines = critical_section::with(|cs| core::mem::take(&mut *LINES.borrow_ref_mut(cs)));
        for line in lines {
            upstream.send_debug(line);
        }
    }

    /// A fixed 2-of-3 share at index 1 and its wallet.
    fn share_and_key() -> (SecretShare, SharedKey) {
        let poly: Vec<Scalar<Secret, Zero>> = [0x5au8, 0xa5]
            .map(|n| Scalar::from_bytes_mod_order([n; 32]))
            .to_vec();
        let index = Scalar::<Public, Zero>::from(1u32)
            .non_zero()
            .expect("1 isn't zero");
        let share = SecretShare {
            index,
            share: poly::scalar::eval(&poly, index),
        };
        let key = SharedKey::from_poly(poly::scalar::to_point_poly(&poly))
            .non_zero()
            .expect("not zero");
        (share, key)
    }

    /// The boot benches: one encode, slice by slice, then the worst repairs.
    #[inline(never)]
    pub fn boot_bench() {
        let (share, key) = share_and_key();

        let started = Instant::now();
        let mut job = Box::new(codec::EncodeJob::new(&share, &key).expect("a valid share"));
        let (mut slices, mut longest) = (0u32, 0u64);
        let card = loop {
            let slice = Instant::now();
            let done = job.step();
            slices += 1;
            longest = longest.max(slice.elapsed().as_millis());
            if let Some(card) = done {
                break card.expect("the fixed share has a card");
            }
        };
        keep(&format!(
            "boot encode: {}ms in {slices} slices (longest {longest}ms)",
            started.elapsed().as_millis()
        ));

        // 2 wrong words at the end of the card, then the same with word 1 unreadable, with the
        // wallet's share indices as the app sends them (build plan A6); then the second case
        // with no index set, which may run into the decode cap and report DontFit
        let wallet_indices: &[u8] = &[1, 2, 3];
        for (unreadable, indices) in [
            (false, Some(wallet_indices)),
            (true, Some(wallet_indices)),
            (true, None),
        ] {
            let mut entries: [Option<u16>; WORDS] = card.words.map(Some);
            for j in [WORDS - 2, WORDS - 1] {
                entries[j] = entries[j].map(|word| (word + 1) % 2048);
            }
            if unreadable {
                entries[0] = None;
            }
            let entries = CardEntries(entries);
            // the decode is constant work until T_dec (owner R2-1): the line gives when the
            // search was done, and the shortest and longest slice, which should be close
            let started = Instant::now();
            let mut job = RestoreJob::new(&entries, Some(&key), None, indices);
            let (mut slices, mut longest, mut shortest) = (0u32, 0u64, u64::MAX);
            let mut search_done = None;
            let outcome = loop {
                let slice = Instant::now();
                let done = job.step(started.elapsed().as_millis());
                slices += 1;
                let took = slice.elapsed().as_micros();
                longest = longest.max(took);
                shortest = shortest.min(took);
                if search_done.is_none() && job.search_done() {
                    search_done = Some(started.elapsed().as_millis());
                }
                if let Some(outcome) = done {
                    break outcome;
                }
            };
            let restored = matches!(
                &outcome,
                RestoreOutcome::Decoded(decoded) if decoded.share.share == share.share
            );
            keep(&format!(
                "boot repair e=2 f={} {}: {}ms in {slices} slices (shortest {shortest}us, longest {longest}us), search done at {search_done:?}ms, restored {restored}",
                unreadable as u8,
                if indices.is_some() { "set {1,2,3}" } else { "no set" },
                started.elapsed().as_millis()
            ));
        }
    }
}
