#![allow(unused)]
use crate::api;
use crate::api::backup_run::{BackupDevice, BackupRun};
use crate::api::coordinator::KeyState;
use crate::api::device_list::DeviceListUpdate;
use crate::device_list::DeviceList;
use crate::frb_generated::StreamSink;
use anyhow::{anyhow, Result};
use frostsnap_coordinator::backup_run::BackupState;
use frostsnap_coordinator::card_backup::{
    check_card_firmware, leave_card_flow, persist_card_sends, unheard_verdict, CardBackupProtocol,
    CardFlowState, CardInbox, CardPersistError,
};
use frostsnap_coordinator::check_backup::{CheckBackupProtocol, CheckBackupState};
use frostsnap_coordinator::enter_physical_backup::{
    CardPhoneHalfHandle, EnterPhysicalBackup, EnterPhysicalBackupState,
};
use frostsnap_coordinator::erase_device::{EraseDevice, EraseDeviceState};
use frostsnap_coordinator::firmware_upgrade::{
    FirmwareUpgradeConfirmState, FirmwareUpgradeProtocol,
};
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination, Sha256Digest,
};
use frostsnap_coordinator::frostsnap_persist::DeviceNames;
use frostsnap_coordinator::genuine_check::GenuineCheck;
use frostsnap_coordinator::nonce_replenish::NonceReplenishState;
use frostsnap_coordinator::persist::{Persist, Persisted, TakeStaged};
use frostsnap_coordinator::signing::SigningState;
use frostsnap_coordinator::verify_address::{VerifyAddressProtocol, VerifyAddressProtocolState};
use frostsnap_coordinator::wait_for_single_device::{
    WaitForSingleDevice, WaitForSingleDeviceState,
};
use frostsnap_coordinator::{
    AppMessageBody, DeviceChange, DeviceMode, FirmwareVersion, Sink, UiProtocol, UiStack,
    UsbSender, UsbSerialManager, ValidatedFirmwareBin, WaitForToUserMessage,
};
use frostsnap_core::coordinator::card::{
    BackupFormat, BoxAnswer, CardActionError, CardEvent, CardToUser, CARD_FORMAT,
};
use frostsnap_core::coordinator::restoration::{
    PhysicalBackupPhase, RecoverShare, RestorationState, ToUserRestoration,
};
use frostsnap_core::coordinator::{
    BeginKeygen, CoordAccessStructure, CoordFrostKey, CoordinatorSend, CoordinatorToUserMessage,
    FrostCoordinator, NonceReplenishRequest,
};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::DeviceToCoordinatorMessage;
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use frostsnap_core::{
    message, schnorr_fun::frost::ShareIndex, AccessStructureRef, DeviceId, KeyId, KeygenId,
    RestorationId, SignSessionId, SymmetricKey, WireSignTask,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::ops::DerefMut;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{event, Level};
const N_NONCE_STREAMS: usize = 4;

pub struct FfiCoordinator {
    usb_manager: Mutex<Option<UsbSerialManager>>,
    key_event_stream: Arc<Mutex<Option<Box<dyn Sink<KeyState>>>>>,
    signing_session_signals: Arc<Mutex<HashMap<KeyId, Signal>>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    ui_stack: Arc<Mutex<UiStack>>,
    pub(crate) usb_sender: UsbSender,
    firmware_bin: Option<ValidatedFirmwareBin>,
    firmware_upgrade_progress: Arc<Mutex<Option<Box<dyn Sink<f32>>>>>,
    device_list: Arc<Mutex<DeviceList>>,
    device_list_stream: Arc<Mutex<Option<Box<dyn Sink<DeviceListUpdate>>>>>,
    // // persisted things
    pub(crate) db: Arc<Mutex<rusqlite::Connection>>,
    device_names: Arc<Mutex<Persisted<DeviceNames>>>,
    /// `None` in a build with no factory key.
    genuine_check: Option<Arc<Mutex<Persisted<GenuineCheck>>>>,
    pub(crate) coordinator: Arc<Mutex<Persisted<FrostCoordinator>>>,
    // backup management
    pub(crate) backup_state: Arc<Mutex<Persisted<BackupState>>>,
    pub(crate) backup_run_streams: Arc<Mutex<BTreeMap<KeyId, StreamSink<BackupRun>>>>,
    /// Each card restore's words 33–64, by device: the app's entry sets them.
    card_phone_halves: Mutex<BTreeMap<DeviceId, CardPhoneHalfHandle>>,
}

type Signal = Box<dyn Sink<()>>;

impl FfiCoordinator {
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        usb_manager: UsbSerialManager,
        genuine_cert_key: Option<Point<EvenY>>,
    ) -> anyhow::Result<Self> {
        let mut db_ = db.lock().unwrap();

        event!(Level::DEBUG, "loading core coordinator");
        let coordinator = Persisted::<FrostCoordinator>::new(&mut db_, ())?;
        event!(Level::DEBUG, "loading device names");
        let device_names = Persisted::<DeviceNames>::new(&mut db_, ())?;
        let genuine_check = match genuine_cert_key {
            Some(factory_key) => {
                event!(Level::DEBUG, "loading genuine certificates");
                let check = Persisted::<GenuineCheck>::new(&mut db_, factory_key)?;
                Some(Arc::new(Mutex::new(check)))
            }
            None => None,
        };
        event!(Level::DEBUG, "loading backup state");
        let backup_state = Persisted::<BackupState>::new(&mut db_, ())?;

        let usb_sender = usb_manager.usb_sender();
        let firmware_bin = usb_manager.upgrade_bin();

        let usb_manager = Mutex::new(Some(usb_manager));
        drop(db_);

        Ok(Self {
            usb_manager,
            thread_handle: Default::default(),
            key_event_stream: Default::default(),
            signing_session_signals: Default::default(),
            ui_stack: Default::default(),
            firmware_upgrade_progress: Default::default(),
            device_list: Default::default(),
            device_list_stream: Default::default(),
            usb_sender,
            firmware_bin,
            db,
            coordinator: Arc::new(Mutex::new(coordinator)),
            device_names: Arc::new(Mutex::new(device_names)),
            genuine_check,
            backup_state: Arc::new(Mutex::new(backup_state)),
            backup_run_streams: Default::default(),
            card_phone_halves: Default::default(),
        })
    }

    pub fn inner(&self) -> impl DerefMut<Target = Persisted<FrostCoordinator>> + '_ {
        self.coordinator.lock().unwrap()
    }

    pub fn start(&self) -> anyhow::Result<()> {
        assert!(
            self.thread_handle.lock().unwrap().is_none(),
            "can't start coordinator thread again"
        );

        let mut usb_manager = self
            .usb_manager
            .lock()
            .unwrap()
            .take()
            .expect("can only start once");
        let coordinator_loop = self.coordinator.clone();
        let ui_stack = self.ui_stack.clone();
        let db_loop = self.db.clone();
        let device_names = self.device_names.clone();
        let usb_sender = self.usb_sender.clone();
        let firmware_upgrade_progress = self.firmware_upgrade_progress.clone();
        let device_list = self.device_list.clone();
        let device_list_stream = self.device_list_stream.clone();
        let genuine_check = self.genuine_check.clone();
        let backup_state = self.backup_state.clone();
        let backup_run_streams = self.backup_run_streams.clone();

        let handle = std::thread::spawn(move || {
            loop {
                // to give time for the other threads to get a lock
                std::thread::sleep(Duration::from_millis(100));

                // check for firmware upgrade mode before locking anything else
                let mut firmware_upgrade_progress_loop = firmware_upgrade_progress.lock().unwrap();
                if let Some(firmware_upgrade_pogress) = &mut *firmware_upgrade_progress_loop {
                    // We're in a firmware upgrade.
                    // Do the firmware upgrade and then carry on as usual
                    let mut error = Ok(());
                    match usb_manager.run_firmware_upgrade() {
                        Ok(progress_iter) => {
                            for progress in progress_iter {
                                match progress {
                                    Ok(progress) => {
                                        firmware_upgrade_pogress.send(progress);
                                    }
                                    Err(e) => {
                                        error = Err(e);
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error = Err(e);
                        }
                    }

                    *firmware_upgrade_progress_loop = None;
                    match error {
                        Ok(_) => {
                            event!(Level::INFO, "firmware upgrade completed")
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                error = e.to_string(),
                                "firmware upgrade error'd out"
                            );
                        }
                    }
                }

                // NOTE: Never hold locks on anything over poll_ports because poll ports makes
                // blocking calls up to flutter. If flutter is blocked on something else we'll
                // be deadlocked.
                let device_changes = usb_manager.poll_ports();
                let mut coordinator = coordinator_loop.lock().unwrap();
                let mut coordinator_outbox = VecDeque::default();
                let mut messages_from_devices = vec![];
                let mut card_inbox = CardInbox::default();
                let mut db = db_loop.lock().unwrap();
                let mut ui_stack = ui_stack.lock().unwrap();
                let mut genuine = genuine_check.as_ref().map(|g| g.lock().unwrap());

                // process new messages from devices
                {
                    let mut device_list = device_list.lock().unwrap();
                    for change in device_changes {
                        device_list.consume_manager_event(change.clone());
                        match change {
                            DeviceChange::Connected {
                                id,
                                firmware_digest,
                                ..
                            } => {
                                if let Some(g) = &mut genuine {
                                    if let Some(request) = g.MUTATE_NO_PERSIST().connected(
                                        id,
                                        FirmwareVersion::new(firmware_digest),
                                        &mut rand::thread_rng(),
                                    ) {
                                        usb_sender
                                            .send(CoordinatorSendMessage::to(id, request.into()));
                                    }
                                }
                            }
                            DeviceChange::Registered { id, .. } => {
                                let pending = coordinator.pending_physical_consolidations(id);
                                if !pending.is_empty() {
                                    device_list.set_recovery_mode(
                                        id,
                                        api::device_list::RecoveryMode::On { pending },
                                    );
                                }

                                let device =
                                    device_list.get_device(id).expect("it was just registered");
                                // A card flow started after N without its device sends its
                                // `Begin` only to the bundled firmware (design §6.6): otherwise
                                // it waits on its own page, or its question (owner B9), sending
                                // nothing.
                                if check_card_firmware(
                                    &device.firmware,
                                    device.latest_firmware.as_ref(),
                                )
                                .is_err()
                                {
                                    if let Some(card) = ui_stack
                                        .get_mut::<CardBackupProtocol>()
                                        .filter(|card| card.device_id() == id)
                                    {
                                        card.firmware_refused(id);
                                    }
                                }
                                ui_stack.connected(id, device.device_mode());

                                if let Some(connected_device) = device_list.get_device(id) {
                                    // we only send some messages out if the device has up to date firmware
                                    if connected_device.firmware_is_up_to_date() {
                                        // coordinator_outbox.extend(
                                        //     coordinator.maybe_request_nonce_replenishment(
                                        //         &BTreeSet::from([id]),
                                        //         N_NONCE_STREAMS,
                                        //         &mut rand::thread_rng(),
                                        //     ),
                                        // );
                                    }
                                }
                            }
                            DeviceChange::Disconnected { id } => {
                                // A card question waits for the device's next gate screen.
                                // Owner B9: while a card check runs, the interruption is
                                // persisted before its question goes to the flow.
                                coordinator_outbox.extend(card_inbox.disconnected(
                                    &mut coordinator,
                                    &mut *db,
                                    id,
                                ));
                                ui_stack.disconnected(id);
                                if let Some(g) = &mut genuine {
                                    g.MUTATE_NO_PERSIST().disconnected(id);
                                }
                            }
                            DeviceChange::NameChange { id, name } => {
                                let mut device_names = device_names.lock().unwrap();
                                // TODO: Detect name change and prompt user to accept
                                let result = device_names.staged_mutate(&mut *db, |names| {
                                    names.insert(id, name.clone());
                                    Ok(())
                                });

                                match result {
                                    Err(e) => {
                                        event!(
                                            Level::ERROR,
                                            id = id.to_string(),
                                            name = name,
                                            error = e.to_string(),
                                            "failed to persist device name change"
                                        );
                                    }
                                    Ok(_) => {
                                        usb_manager.accept_device_name(id, name.clone());
                                    }
                                }
                            }
                            DeviceChange::AppMessage(message) => {
                                messages_from_devices.push(message.clone());
                            }
                            DeviceChange::NeedsName { id } => {
                                ui_stack.connected(id, DeviceMode::Blank);
                            }
                            DeviceChange::GenuineCheck { from, message } => {
                                if let Some(g) = &mut genuine {
                                    let result = g.mutate2(&mut *db, |check, update| {
                                        match check.recv(
                                            from,
                                            message,
                                            update,
                                            &mut rand::thread_rng(),
                                        ) {
                                            Ok(Some(request)) => {
                                                usb_sender.send(CoordinatorSendMessage::to(
                                                    from,
                                                    request.into(),
                                                ));
                                            }
                                            Ok(None) => {}
                                            Err(error) => event!(
                                                Level::WARN,
                                                device = from.to_string(),
                                                error = error.to_string(),
                                                "ignoring a genuine check response that did not verify"
                                            ),
                                        }
                                        Ok(())
                                    });
                                    if let Err(e) = result {
                                        event!(
                                            Level::ERROR,
                                            device = from.to_string(),
                                            error = e.to_string(),
                                            "failed to persist the genuine check"
                                        );
                                    }
                                }
                            }
                        }
                    }

                    if let Some(g) = &mut genuine {
                        for (id, status) in g.MUTATE_NO_PERSIST().take_events() {
                            device_list.set_genuine(id, status);
                        }
                    }

                    if device_list.update_ready() {
                        if let Some(device_list_stream) = &*device_list_stream.lock().unwrap() {
                            device_list_stream.send(device_list.take_update());
                        }
                    }
                };

                for app_message in messages_from_devices {
                    match app_message.body {
                        AppMessageBody::Core(core_message)
                            if matches!(*core_message, DeviceToCoordinatorMessage::Card(_)) =>
                        {
                            // Card messages go out only once what they staged is persisted
                            // (design §2.1 item 5).
                            coordinator_outbox.extend(card_inbox.recv(
                                &mut coordinator,
                                &mut *db,
                                app_message.from,
                                usb_manager.shares_port(app_message.from),
                                *core_message,
                            ));
                        }
                        AppMessageBody::Core(core_message) => {
                            let core_message = *core_message;
                            let result = coordinator.staged_mutate(&mut *db, |coordinator| {
                                match coordinator
                                    .recv_device_message(app_message.from, core_message)
                                {
                                    Ok(messages) => {
                                        coordinator_outbox.extend(messages);
                                    }
                                    Err(e) => {
                                        event!(
                                            Level::ERROR,
                                            from = app_message.from.to_string(),
                                            "Failed to process message: {}",
                                            e
                                        );
                                    }
                                }

                                Ok(())
                            });

                            if let Err(e) = result {
                                event!(
                                    Level::ERROR,
                                    error = e.to_string(),
                                    "failed to persist changes from device message"
                                );
                            }
                        }
                        AppMessageBody::Misc(comms_misc) => {
                            ui_stack.process_comms_message(app_message.from, comms_misc);
                        }
                    }
                }

                // The phone's deadline after N (owner B9): a device that stalls on the work it
                // owes is an abort.
                if let Some(card) = ui_stack.get_mut::<CardBackupProtocol>() {
                    coordinator_outbox.extend(card_inbox.device_timeout(
                        &mut coordinator,
                        &mut *db,
                        card,
                        Instant::now(),
                    ));
                }

                drop(genuine);
                drop(coordinator);
                drop(db);

                while let Some(message) = coordinator_outbox.pop_front() {
                    match message {
                        CoordinatorSend::ToDevice {
                            message,
                            destinations,
                        } => {
                            let send_message = CoordinatorSendMessage {
                                target_destinations: Destination::from(destinations),
                                message_body: CoordinatorSendBody::Core(message),
                            };

                            usb_sender.send(send_message);
                        }
                        CoordinatorSend::ToUser(msg) => {
                            ui_stack.process_to_user_message(msg);
                        }
                    }
                }

                if let Some(card) = ui_stack.get_mut::<CardBackupProtocol>() {
                    card_inbox.apply(card);
                }

                // poll the ui protocol first before locking anything else because of the potential
                // for dead locks with callbacks activated on stream items trying to lock things.
                for message in ui_stack.poll() {
                    usb_sender.send(message);
                }

                if ui_stack.clean_finished() {
                    // the UI stack ahs told us we need to cancel all since one of the protocols
                    // completed with an abort.
                    usb_sender.send_cancel_all();
                }
                drop(ui_stack);

                // A card verdict can come after the card page was left, so the checklist and
                // the wallet banner are updated here too. Locks in `build_backup_run`'s order.
                for key_id in card_inbox.changed_keys() {
                    let backup_run = {
                        let backup_state = backup_state.lock().unwrap();
                        let coordinator = coordinator_loop.lock().unwrap();
                        backup_run(&backup_state, &coordinator, key_id)
                    };
                    if let Some(stream) = backup_run_streams.lock().unwrap().get(&key_id) {
                        let _ = stream.add(backup_run);
                    }
                }
            }
        });

        *self.thread_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn sub_key_events(&self, stream: impl Sink<api::coordinator::KeyState>) {
        let mut key_event_stream = self.key_event_stream.lock().unwrap();
        let state = self.key_state();
        stream.send(state);
        key_event_stream.replace(Box::new(stream));
    }

    pub fn update_name_preview(&self, id: DeviceId, name: &str) -> anyhow::Result<()> {
        let device_name: frostsnap_coordinator::frostsnap_comms::DeviceName = name.try_into()?;
        self.usb_sender.update_name_preview(id, device_name);
        Ok(())
    }

    pub fn send_cancel(&self, id: DeviceId) {
        self.usb_sender.send_cancel(id)
    }

    pub fn generate_new_key(
        &self,
        devices: Vec<DeviceId>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
        sink: impl Sink<frostsnap_coordinator::keygen::KeyGenState>,
    ) -> anyhow::Result<()> {
        let coordinator = self.coordinator.lock().unwrap();
        if devices.iter().any(|&id| coordinator.device_is_suspect(id)) {
            return Err(anyhow!(
                "a selected device failed a backup check, so it can't be used for a new wallet"
            ));
        }
        drop(coordinator);
        // Every new key backs up as the 64-word split card (design §5.4).
        let n_devices = devices.len();
        if !self.devices_support_card(&devices) {
            return Err(anyhow!(
                "a selected device's firmware doesn't support the backup card"
            ));
        }
        // A card holds the index and the threshold in a byte each, and a threshold-1 card
        // only for a single device.
        if n_devices > 255 || threshold > 255 || (threshold == 1 && n_devices != 1) {
            return Err(anyhow!(
                "a {threshold}-of-{n_devices} wallet can't use the backup card"
            ));
        }
        let backup_format = BackupFormat::Card64;

        let device_list = self.device_list.lock().unwrap();
        let devices = devices.into_iter().collect();
        // sort them as connected so we get #1 assigned to the first one etc
        let devices = device_list.sort_as_connected(devices).collect();
        let currently_connected = device_list.devices().into_iter().map(|device| device.id);
        drop(device_list);

        let begin_keygen = BeginKeygen::new(
            devices,
            threshold,
            key_name,
            purpose,
            &mut rand::thread_rng(),
        )
        .with_backup_format(backup_format);

        let ui_protocol = frostsnap_coordinator::keygen::KeyGen::new(
            sink,
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            currently_connected.into_iter().collect(),
            begin_keygen,
            &mut rand::thread_rng(),
        );

        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn frost_keys(&self) -> Vec<CoordFrostKey> {
        self.coordinator
            .lock()
            .unwrap()
            .iter_keys()
            .cloned()
            .collect()
    }

    pub fn nonces_available(&self, id: DeviceId) -> u32 {
        self.coordinator
            .lock()
            .unwrap()
            .nonces_available(id)
            .values()
            .copied()
            .max()
            .unwrap_or(0)
    }

    pub fn nonce_replenish_request(&self, devices: BTreeSet<DeviceId>) -> NonceReplenishRequest {
        self.coordinator
            .lock()
            .unwrap()
            .MUTATE_NO_PERSIST()
            .maybe_request_nonce_replenishment(&devices, N_NONCE_STREAMS, &mut rand::thread_rng())
    }

    pub fn replenish_nonces(
        &self,
        nonce_request: NonceReplenishRequest,
        devices: BTreeSet<DeviceId>,
        sink: impl Sink<frostsnap_coordinator::nonce_replenish::NonceReplenishState>,
    ) -> anyhow::Result<()> {
        let ui_protocol = frostsnap_coordinator::nonce_replenish::NonceReplenishProtocol::new(
            devices,
            nonce_request,
            sink,
        );

        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn start_signing(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: BTreeSet<DeviceId>,
        task: WireSignTask,
        sink: impl Sink<SigningState>,
    ) -> anyhow::Result<()> {
        let mut coordinator = self.coordinator.lock().unwrap();
        let session_id =
            coordinator.staged_mutate(&mut self.db.lock().unwrap(), |coordinator| {
                Ok(coordinator.start_sign(
                    access_structure_ref,
                    task,
                    &devices,
                    &mut rand::thread_rng(),
                )?)
            })?;

        let signals = self.signing_session_signals.clone();
        let sink = sink.inspect(move |_| {
            if let Some(signal_sink) = signals.lock().unwrap().get(&access_structure_ref.key_id) {
                signal_sink.send(());
            }
        });

        let mut ui_protocol = frostsnap_coordinator::signing::SigningDispatcher::new(
            devices,
            access_structure_ref.key_id,
            session_id,
            sink,
        );
        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn request_device_sign(
        &self,
        device_id: DeviceId,
        session_id: SignSessionId,
        encryption_key: SymmetricKey,
    ) -> anyhow::Result<()> {
        let mut ui_stack = self.ui_stack.lock().unwrap();

        let signing = ui_stack
            .get_mut::<frostsnap_coordinator::signing::SigningDispatcher>()
            .ok_or(anyhow!("somehow UI was not in KeyGen state"))?;

        let mut db = self.db.lock().unwrap();

        let sign_req = self
            .coordinator
            .lock()
            .unwrap()
            .staged_mutate(&mut *db, |coordinator| {
                Ok(coordinator.request_device_sign(session_id, device_id, encryption_key))
            })?;

        signing.send_sign_request(sign_req);

        Ok(())
    }

    pub fn try_restore_signing_session(
        &self,
        session_id: SignSessionId,
        sink: impl Sink<SigningState>,
    ) -> anyhow::Result<()> {
        let coordinator = self.coordinator.lock().unwrap();

        let active_sign_session = coordinator
            .active_signing_sessions_by_ssid()
            .get(&session_id)
            .ok_or(anyhow!("this signing session no longer exists"))?;

        let key_id = active_sign_session.key_id;

        let signals = self.signing_session_signals.clone();
        let sink = sink.inspect(move |_| {
            if let Some(signal_sink) = signals.lock().unwrap().get(&key_id) {
                signal_sink.send(());
            }
        });

        let mut dispatcher =
            frostsnap_coordinator::signing::SigningDispatcher::restore_signing_session(
                active_sign_session,
                sink,
            );
        dispatcher.emit_state();
        self.start_protocol(dispatcher);
        Ok(())
    }

    pub fn begin_upgrade_firmware(
        &self,
        sink: impl Sink<FirmwareUpgradeConfirmState>,
    ) -> anyhow::Result<()> {
        let firmware_bin = self.firmware_bin.ok_or(anyhow!(
            "App wasn't compiled with BUNDLE_FIRMWARE so it can't do firmware upgrades"
        ))?;

        let ui_protocol = {
            let device_list = self.device_list.lock().unwrap();

            let devices: HashMap<DeviceId, FirmwareVersion> = device_list
                .devices()
                .into_iter()
                .map(|device| (device.id, device.firmware))
                .collect();

            let need_upgrade = device_list
                .devices()
                .into_iter()
                .filter(|device| device.needs_firmware_upgrade())
                .map(|device| device.id)
                .collect();

            let ui_protocol =
                FirmwareUpgradeProtocol::new(devices, need_upgrade, firmware_bin, sink);
            ui_protocol.emit_state();
            ui_protocol
        };
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn upgrade_firmware_digest(&self) -> Option<Sha256Digest> {
        self.firmware_bin.map(|firmware_bin| firmware_bin.digest())
    }

    pub fn upgrade_firmware_version_name(&self) -> Option<String> {
        self.firmware_bin
            .map(|firmware_bin| firmware_bin.firmware_version().version_name())
    }

    /// Locks in the loop's order, ui_stack then device_list, and pushes before letting go: a
    /// connect or disconnect can't fall between the device snapshot and the push, unheard.
    pub(crate) fn start_protocol<P: UiProtocol + Send + 'static>(&self, mut protocol: P) {
        let mut stack = self.ui_stack.lock().unwrap();
        for device in self.device_list.lock().unwrap().devices() {
            protocol.connected(device.id, device.device_mode());
        }
        stack.push(protocol);
    }

    pub fn cancel_protocol(&self) {
        // HACK: cancel_protocol only tears down the UI-side protocol wrapper.
        // For keygen we also need to clear FrostCoordinator::pending_keygens,
        // otherwise an in-flight device response (Certify/Ack) arriving after
        // cancel will cause the core to emit a follow-up keygen message that
        // races the wire Cancel and panics the device. The proper fix is to
        // replace cancel_protocol with per-protocol handles that own their
        // own cancel logic; until then we introspect the stack here.
        let mut coordinator = self.coordinator.lock().unwrap();
        let mut ui_stack = self.ui_stack.lock().unwrap();
        let keygen_id = ui_stack
            .get_mut::<frostsnap_coordinator::keygen::KeyGen>()
            .map(|k| k.keygen_id());
        if ui_stack.cancel_all() {
            if let Some(keygen_id) = keygen_id {
                coordinator.MUTATE_NO_PERSIST().cancel_keygen(keygen_id);
            }
            drop(ui_stack);
            drop(coordinator);
            self.usb_sender.send_cancel_all();
        }
    }

    pub fn enter_firmware_upgrade_mode(&self, sink: impl Sink<f32>) -> Result<()> {
        match &mut *self.firmware_upgrade_progress.lock().unwrap() {
            Some(_) => {
                event!(
                    Level::ERROR,
                    "tried to enter firmware upgrade mode while we were already in an upgrade"
                );
                return Err(anyhow!(
                    "trierd to enter firmware upgrade mode while already in an upgrade"
                ));
            }
            progress => *progress = Some(Box::new(sink)),
        }
        Ok(())
    }

    pub fn get_device_name(&self, id: DeviceId) -> Option<String> {
        self.device_names.lock().unwrap().get(id)
    }

    pub fn get_device_case_color(&self, id: DeviceId) -> Option<api::device_list::CaseColor> {
        let genuine_check = self.genuine_check.as_ref()?.lock().unwrap();
        Some(genuine_check.certificate(id)?.case_color())
    }

    pub fn genuine_check_enabled(&self) -> bool {
        self.genuine_check.is_some()
    }

    /// Persist a user-typed device name into the coord's local `device_names` store without
    /// waiting for the device's `SetName` round-trip. Stores verbatim — name hygiene (trim,
    /// charset, length) is the Dart input layer's responsibility; the keygen and recovery
    /// `TextField`s enforce it via `maxLength: DeviceName.maxLength()` and `nameInputFormatter`,
    /// and the Dart-side commit paths trim before submitting. The device's eventual `SetName`
    /// remains the durable source of truth.
    fn commit_device_name_locally(&self, db: &mut rusqlite::Connection, id: DeviceId, name: &str) {
        let mut device_names = self.device_names.lock().unwrap();
        if let Err(e) = device_names.staged_mutate(db, |names| {
            names.insert(id, name.to_string());
            Ok(())
        }) {
            event!(
                Level::ERROR,
                id = id.to_string(),
                name = name,
                error = e.to_string(),
                "failed to persist device name locally"
            );
        }
    }

    pub fn finalize_keygen(
        &self,
        keygen_id: KeygenId,
        symmetric_key: SymmetricKey,
        device_names: Vec<crate::api::keygen::DeviceNameCommit>,
    ) -> Result<AccessStructureRef> {
        let access_structure_ref = {
            let mut coordinator = self.coordinator.lock().unwrap();
            let mut db = self.db.lock().unwrap();
            let mut ui_stack = self.ui_stack.lock().unwrap();
            let keygen = ui_stack
                .get_mut::<frostsnap_coordinator::keygen::KeyGen>()
                .ok_or(anyhow!("somehow UI was not in KeyGen state"))?;

            let participant_set: BTreeSet<DeviceId> = keygen.devices().iter().copied().collect();

            let finalized_keygen = coordinator.staged_mutate(&mut db, |coordinator| {
                Ok(coordinator.finalize_keygen(
                    keygen_id,
                    symmetric_key,
                    &mut rand::thread_rng(),
                )?)
            })?;
            let access_structure_ref = finalized_keygen.access_structure_ref;

            self.usb_sender.send_from_core(finalized_keygen);

            for commit in device_names {
                if !participant_set.contains(&commit.id) {
                    event!(
                        Level::DEBUG,
                        id = commit.id.to_string(),
                        "ignoring device name commit for non-participant"
                    );
                    continue;
                }
                self.commit_device_name_locally(&mut db, commit.id, &commit.name);
            }

            keygen.keygen_finalized(access_structure_ref);
            access_structure_ref
        };

        self.emit_key_state();

        // Start backup run for newly created wallet
        {
            let coordinator = self.coordinator.lock().unwrap();
            let access_structure = coordinator
                .get_access_structure(access_structure_ref)
                .expect("access structure must exist after keygen");
            let share_indices: Vec<u32> = access_structure
                .iter_shares()
                .map(|(_, share_index)| {
                    u32::try_from(share_index).expect("share index should fit in u32")
                })
                .collect();
            drop(coordinator);

            let mut backup_state = self.backup_state.lock().unwrap();
            let mut db = self.db.lock().unwrap();
            backup_state.mutate2(&mut *db, |state, mutations| {
                state.start_run(access_structure_ref, share_indices, mutations);
                Ok(())
            })?;
        }

        // Emit backup stream
        let _ = self.backup_stream_emit(access_structure_ref.key_id);

        Ok(access_structure_ref)
    }

    pub fn get_access_structure(&self, as_ref: AccessStructureRef) -> Option<CoordAccessStructure> {
        self.coordinator
            .lock()
            .unwrap()
            .get_access_structure(as_ref)
    }

    pub fn get_frost_key(&self, key_id: KeyId) -> Option<CoordFrostKey> {
        self.coordinator
            .lock()
            .unwrap()
            .get_frost_key(key_id)
            .cloned()
    }

    pub fn verify_address(
        &self,
        key_id: KeyId,
        address_index: u32,
        stream: impl Sink<VerifyAddressProtocolState>,
    ) -> anyhow::Result<()> {
        let coordinator = self.coordinator.lock().unwrap();

        let address_index = frostsnap_core::tweak::NormalIndex::new(address_index)
            .ok_or_else(|| anyhow!("address index {address_index} is not a normal bip32 child"))?;

        let verify_address_messages = coordinator.verify_address(key_id, address_index)?;

        let ui_protocol = VerifyAddressProtocol::new(verify_address_messages.clone(), stream);

        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn key_state(&self) -> api::coordinator::KeyState {
        key_state(&self.coordinator.lock().unwrap())
    }

    pub fn wait_for_single_device(&self, sink: impl Sink<WaitForSingleDeviceState>) {
        let mut ui_protocol = WaitForSingleDevice::new(sink);
        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);
    }

    pub fn start_restoring_wallet(
        &self,
        name: String,
        threshold: Option<u16>,
        key_purpose: KeyPurpose,
    ) -> Result<RestorationId> {
        let restoration_id = {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                let restoration_id = RestorationId::new(&mut rand::thread_rng());
                coordinator.start_restoring_key(name, threshold, key_purpose, restoration_id);
                Ok(restoration_id)
            })?
        };

        self.emit_key_state();

        Ok(restoration_id)
    }

    pub fn start_restoring_wallet_from_device_share(
        &self,
        recover_share: &RecoverShare,
    ) -> Result<RestorationId> {
        let restoration_id = {
            let mut coordinator = self.coordinator.lock().unwrap();

            if let Some(access_structure_ref) = recover_share.held_share.access_structure_ref {
                if coordinator
                    .get_access_structure(access_structure_ref)
                    .is_some()
                {
                    return Err(anyhow!("we already know about this access structure"));
                }
            }
            let mut db = self.db.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                let restoration_id = RestorationId::new(&mut rand::thread_rng());
                coordinator.start_restoring_key_from_recover_share(recover_share, restoration_id);
                Ok(restoration_id)
            })?
        };

        self.emit_key_state();

        Ok(restoration_id)
    }

    pub fn continue_restoring_wallet_from_device_share(
        &self,
        restoration_id: RestorationId,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();

            coordinator.staged_mutate(&mut *db, |coordinator| {
                coordinator.add_recovery_share_to_restoration(
                    restoration_id,
                    recover_share,
                    encryption_key,
                )?;
                Ok(())
            })?;
        }

        self.emit_key_state();
        Ok(())
    }

    pub fn recover_share(
        &self,
        access_structure_ref: AccessStructureRef,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        let held_by = recover_share.held_by;
        {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                coordinator.recover_share(access_structure_ref, recover_share, encryption_key)?;
                Ok(())
            })?;
        }

        self.exit_recovery_mode(held_by, access_structure_ref, encryption_key);

        self.emit_key_state();

        Ok(())
    }

    pub fn get_restoration_state(&self, restoration_id: RestorationId) -> Option<RestorationState> {
        self.coordinator
            .lock()
            .unwrap()
            .get_restoration_state(restoration_id)
    }

    pub fn finish_restoring(
        &self,
        restoration_id: RestorationId,
        encryption_key: SymmetricKey,
    ) -> Result<AccessStructureRef> {
        let (assid, needs_consolidation) = {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            let restoration_state = coordinator
                .get_restoration_state(restoration_id)
                .ok_or(anyhow!("can't finish restoration that doesn't exist"))?;
            let needs_consolidation: Vec<_> = restoration_state.needs_to_consolidate().collect();
            let assid = coordinator.staged_mutate(&mut *db, |coordinator| {
                Ok(coordinator.finish_restoring(
                    restoration_id,
                    encryption_key,
                    &mut rand::thread_rng(),
                )?)
            })?;

            (assid, needs_consolidation)
        };

        for device_id in needs_consolidation {
            // NOTE: This will only work for the devices that are plugged in otherwise it's a noop
            self.exit_recovery_mode(device_id, assid, encryption_key);
        }

        self.emit_key_state();

        Ok(assid)
    }

    pub fn delete_key(&self, key_id: KeyId) -> Result<()> {
        {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                coordinator.delete_key(key_id);
                Ok(())
            })?;
            drop(coordinator);

            // Clean up backup runs for deleted key
            let mut backup_state = self.backup_state.lock().unwrap();
            backup_state.mutate2(&mut *db, |state, mutations| {
                state.clear_backup_run(key_id, mutations);
                Ok(())
            })?;
        }

        self.emit_key_state();

        Ok(())
    }

    pub fn delete_share(
        &self,
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
    ) -> Result<()> {
        {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                coordinator
                    .delete_share(access_structure_ref, device_id)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                Ok(())
            })?;
        }
        self.emit_key_state();
        Ok(())
    }

    pub fn sub_device_events(&self, new_stream: impl Sink<api::device_list::DeviceListUpdate>) {
        let mut device_list = self.device_list.lock().unwrap();
        let mut device_list_stream = self.device_list_stream.lock().unwrap();
        new_stream.send(device_list.take_update());
        device_list_stream.replace(Box::new(new_stream));
    }

    pub fn device_at_index(&self, index: usize) -> Option<api::device_list::ConnectedDevice> {
        self.device_list.lock().unwrap().device_at_index(index)
    }

    pub fn device_list_state(&self) -> api::device_list::DeviceListState {
        self.device_list.lock().unwrap().state()
    }

    pub fn get_connected_device(&self, id: DeviceId) -> Option<api::device_list::ConnectedDevice> {
        self.device_list.lock().unwrap().get_device(id)
    }

    pub fn erase_device(&self, id: DeviceId, sink: impl Sink<EraseDeviceState>) {
        let ui_protocol = EraseDevice::new(id, sink);
        self.start_protocol(ui_protocol);
    }

    pub fn erase_all_devices(&self) {
        self.usb_sender.erase_all()
    }

    pub fn cancel_sign_session(&self, ssid: SignSessionId) -> anyhow::Result<()> {
        let session = {
            let mut db = self.db.lock().unwrap();
            event!(
                Level::INFO,
                ssid = ssid.to_string(),
                "canceling sign session"
            );
            let mut coord = self.coordinator.lock().unwrap();
            let session = coord.active_signing_sessions_by_ssid().get(&ssid).cloned();
            coord.staged_mutate(&mut *db, |coordinator| {
                coordinator.cancel_sign_session(ssid);
                Ok(())
            })?;
            session
        };
        if let Some(session) = session {
            let key_id = session.key_id;
            self.emit_signing_signal(key_id);
        }
        Ok(())
    }

    pub fn forget_finished_sign_session(&self, ssid: SignSessionId) -> anyhow::Result<()> {
        let deleted_session = {
            let mut db = self.db.lock().unwrap();
            event!(
                Level::INFO,
                ssid = ssid.to_string(),
                "forgetting finished sign session"
            );
            let mut coord = self.coordinator.lock().unwrap();
            coord.staged_mutate(&mut *db, |coordinator| {
                Ok(coordinator.forget_finished_sign_session(ssid))
            })?
        };

        if let Some(session) = deleted_session {
            self.emit_signing_signal(session.key_id);
        }

        Ok(())
    }
    pub fn cancel_restoration(&self, restoration_id: RestorationId) -> anyhow::Result<()> {
        let mut db = self.db.lock().unwrap();
        let mut coordinator = self.coordinator.lock().unwrap();

        coordinator.staged_mutate(&mut *db, |coordinator| {
            coordinator.cancel_restoration(restoration_id);
            Ok(())
        })?;

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.send(key_state(&coordinator));
        }

        Ok(())
    }

    pub fn sub_signing_session_signals(&self, key_id: KeyId, new_stream: impl Sink<()>) {
        // Emit an initial signal immediately so that subscribers (especially BehaviorSubjects on
        // the Dart side) get an initial value. This is important for UI state that needs to show
        // the correct state immediately on app restart (e.g., the "Continue" button when there
        // are unbroadcasted transactions).
        //
        // NOTE: This signal stream design is a bit annoying because we should really just have a
        // stream of signing sessions rather than a signal stream that causes consumers to
        // recompute state.
        new_stream.send(());
        let mut signal_streams = self.signing_session_signals.lock().unwrap();
        signal_streams.insert(key_id, Box::new(new_stream));
    }

    pub fn emit_signing_signal(&self, key_id: KeyId) {
        let signal_streams = self.signing_session_signals.lock().unwrap();
        if let Some(stream) = signal_streams.get(&key_id) {
            stream.send(())
        }
    }

    pub fn tell_device_to_enter_physical_backup(
        &self,
        device_id: DeviceId,
        sink: impl Sink<EnterPhysicalBackupState>,
    ) -> Result<()> {
        let proto = EnterPhysicalBackup::new(self.track_backup_saved(device_id, sink), device_id);
        self.start_protocol(proto);
        Ok(())
    }

    /// Enter a 64-word card: words 1–32 on the device, words 33–64 from the app through
    /// [`card_restore_phone_half`](Self::card_restore_phone_half) (design §3.7). Downstream
    /// it's the same as a 25-word backup.
    pub fn tell_device_to_enter_card(
        &self,
        device_id: DeviceId,
        target: api::verifiable_card::CardRestoreTarget,
        sink: impl Sink<EnterPhysicalBackupState>,
    ) -> Result<()> {
        use api::verifiable_card::CardRestoreTarget;
        self.check_card_firmware(device_id)?;
        // The indices the key's backup checklist started with. An erased or removed device's
        // share leaves the wallet's map but stays here, and its card is the one most likely to
        // be restored into the wallet. Read before the coordinator, in `build_backup_run`'s lock
        // order.
        let issued: Vec<u32> = match &target {
            CardRestoreTarget::Wallet {
                access_structure_ref,
                ..
            } => self
                .backup_state
                .lock()
                .unwrap()
                .backup_run_share_indices(*access_structure_ref),
            _ => vec![],
        };
        let (target, recorded_format, share_indices) = {
            let mut coordinator = self.coordinator.lock().unwrap();
            match target {
                CardRestoreTarget::None => (None, None, None),
                CardRestoreTarget::Wallet {
                    access_structure_ref,
                    encryption_key,
                } => {
                    let root_shared_key = coordinator
                        .root_shared_key(access_structure_ref, encryption_key)
                        .ok_or(anyhow!("couldn't decrypt the wallet's key"))?;
                    let recorded_format = card_recorded_format(
                        coordinator.key_backup_format(access_structure_ref.key_id),
                    );
                    // The wallet's share indices, and those its checklist started with, keep a
                    // repair within its time (build plan A6). They only filter repairs: a card
                    // at an index they leave out still decodes, and every share is checked
                    // against the key.
                    let share_indices = coordinator.get_access_structure(access_structure_ref).map(
                        |access_structure| {
                            wallet_card_share_indices(
                                access_structure.device_to_share_indicies().into_values(),
                                issued,
                            )
                        },
                    );
                    (Some(root_shared_key), recorded_format, share_indices)
                }
                CardRestoreTarget::Restoration { restoration_id } => {
                    let restoration = coordinator
                        .get_restoration_state(restoration_id)
                        .ok_or(anyhow!("no such restoration"))?;
                    (restoration.access_structure.shared_key, None, None)
                }
            }
        };
        let (proto, phone_half) = EnterPhysicalBackup::new_card(
            self.track_backup_saved(device_id, sink),
            device_id,
            target,
            recorded_format,
            share_indices,
        );
        // A new restore on the device replaces the last one's words.
        self.card_phone_halves
            .lock()
            .unwrap()
            .insert(device_id, phone_half);
        self.start_protocol(proto);
        Ok(())
    }

    /// The app's words 33–64 for the card restore on `device_id`. The restore's protocol sends
    /// them once the device has acknowledged it, and again after each call.
    pub fn card_restore_phone_half(
        &self,
        device_id: DeviceId,
        words: [Option<u16>; 32],
    ) -> Result<()> {
        let running = self
            .ui_stack
            .lock()
            .unwrap()
            .get_mut::<EnterPhysicalBackup>()
            .is_some_and(|restore| restore.card_device() == Some(device_id));
        let halves = self.card_phone_halves.lock().unwrap();
        match halves.get(&device_id) {
            Some(handle) if running => {
                handle.set(words);
                Ok(())
            }
            _ => Err(anyhow!("no card restore is running on this device")),
        }
    }

    fn track_backup_saved(
        &self,
        device_id: DeviceId,
        sink: impl Sink<EnterPhysicalBackupState>,
    ) -> impl Sink<EnterPhysicalBackupState> {
        let device_list = self.device_list.clone();
        let coordinator = self.coordinator.clone();
        let key_event_stream = self.key_event_stream.clone();
        sink.inspect(move |state| {
            if state.saved {
                // When/if the physical backup has been saved on the device this means the device
                // has entered recovery mode. We need to set recovery mode so that the device can be
                // brought out of recovery mode when the time comes.
                device_list.lock().unwrap().set_recovery_mode(
                    device_id,
                    api::device_list::RecoveryMode::On { pending: vec![] },
                );

                let coordinator = coordinator.lock().unwrap();
                // We need to update the recovering key's state the ui gets updated
                let state = key_state(&coordinator);
                if let Some(key_event_stream) = &*key_event_stream.lock().unwrap() {
                    key_event_stream.send(state);
                }
            }
        })
    }

    pub fn tell_device_to_check_backup(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        encryption_key: SymmetricKey,
        sink: impl Sink<CheckBackupState>,
    ) -> Result<()> {
        if self.key_uses_card(access_structure_ref.key_id) {
            return Err(anyhow!(
                "this wallet's backups are verifiable cards, which are checked in the card flow"
            ));
        }
        let firmware = self
            .device_list
            .lock()
            .unwrap()
            .devices()
            .into_iter()
            .find(|d| d.id == device_id)
            .map(|d| d.firmware)
            .ok_or_else(|| anyhow!("device not connected"))?;

        let proto = CheckBackupProtocol::new(
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            device_id,
            access_structure_ref,
            share_index,
            encryption_key,
            firmware,
            sink,
        )?;
        self.start_protocol(proto);
        Ok(())
    }

    // XXX: Cannot be called during another UI protocol
    pub fn tell_device_to_save_physical_backup(
        &self,
        phase: PhysicalBackupPhase,
        restoration_id: RestorationId,
        device_name: String,
    ) {
        {
            let mut coord = self.coordinator.lock().unwrap();
            let messages = coord
                .MUTATE_NO_PERSIST()
                .tell_device_to_save_physical_backup(phase, restoration_id);
            self.usb_sender.send_from_core(messages);
        }

        {
            let mut db = self.db.lock().unwrap();
            self.commit_device_name_locally(&mut db, phase.from, &device_name);
        }

        // hook into to user messages to see when it is successfully saved
        let success = self.block_for_to_user_message([phase.from], move |to_user| {
            if let &CoordinatorToUserMessage::Restoration(
                ToUserRestoration::PhysicalBackupSaved {
                    device_id,
                    restoration_id: rid,
                    share_index,
                },
            ) = &to_user
            {
                return device_id == phase.from
                    && rid == restoration_id
                    && share_index == phase.backup.share_image.index;
            }
            false
        });
        if success {
            self.device_list.lock().unwrap().set_recovery_mode(
                phase.from,
                api::device_list::RecoveryMode::On { pending: vec![] },
            );

            self.emit_key_state();
        }
    }

    /// This is for telling a device that a physical backup it just entered is ready to be used
    /// right away. It never enters recovery mode.
    // XXX: Cannot be called during another UI protocol
    pub fn tell_device_to_consolidate_physical_backup(
        &self,
        access_structure_ref: AccessStructureRef,
        phase: PhysicalBackupPhase,
        encryption_key: SymmetricKey,
        device_name: String,
    ) -> Result<()> {
        let msgs = {
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator
                .MUTATE_NO_PERSIST()
                .tell_device_to_consolidate_physical_backup(
                    phase,
                    access_structure_ref,
                    encryption_key,
                )?
        };

        self.usb_sender.send_from_core(msgs);

        {
            let mut db = self.db.lock().unwrap();
            self.commit_device_name_locally(&mut db, phase.from, &device_name);
        }

        // hook into to user messages to see when it is successfully consolidated
        let success = self.block_for_to_user_message([phase.from], move |to_user| {
            if let &CoordinatorToUserMessage::Restoration(
                ToUserRestoration::FinishedConsolidation {
                    device_id,
                    access_structure_ref: assid,
                    share_index,
                },
            ) = &to_user
            {
                return device_id == phase.from
                    && assid == access_structure_ref
                    && share_index == phase.backup.share_image.index;
            }
            false
        });

        if success {
            self.emit_key_state();
        }

        Ok(())
    }

    fn block_for_to_user_message(
        &self,
        devices: impl IntoIterator<Item = DeviceId>,
        f: impl FnMut(CoordinatorToUserMessage) -> bool + Send + 'static,
    ) -> bool {
        let (proto, waiter) = WaitForToUserMessage::new(devices, f);
        self.start_protocol(proto);

        waiter.recv().expect("unreachable")
    }

    /// i.e. do a consolidation
    pub fn exit_recovery_mode(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
    ) {
        let device = match self.device_list.lock().unwrap().get_device(device_id) {
            Some(device) => device,
            None => return,
        };

        let msg = {
            let mut coord = self.coordinator.lock().unwrap();
            match coord
                .MUTATE_NO_PERSIST()
                .consolidate_pending_physical_backups(
                    device_id,
                    access_structure_ref,
                    encryption_key,
                ) {
                Ok(Some(msg)) => msg,
                // Nothing pending for this wallet on this device; there is no exit to wait for.
                Ok(None) => return,
                Err(e) => {
                    event!(
                        Level::ERROR,
                        id = device_id.to_string(),
                        name = device.name,
                        error = e.to_string(),
                        "unable to consolidate backups to take device out of recovery mode"
                    );
                    return;
                }
            }
        };

        self.usb_sender.send_from_core([msg]);

        event!(
            Level::INFO,
            id = device_id.to_string(),
            name = device.name,
            "asking device to exit recovery mode"
        );

        // Wait for every share of THIS wallet to finish. Matching device + ref (not
        // device alone) keeps a concurrent per-ref exit from satisfying this waiter.
        let success = self.block_for_to_user_message([device_id], move |to_user| {
            matches!(
                to_user,
                CoordinatorToUserMessage::Restoration(ToUserRestoration::FinishedConsolidation {
                    device_id: got,
                    access_structure_ref: got_ref,
                    ..
                }) if got == device_id && got_ref == access_structure_ref
            )
        });

        if !success {
            event!(
                Level::ERROR,
                id = device_id.to_string(),
                name = device.name,
                "device failed to exit recovery mode"
            );
            return;
        }

        event!(
            Level::INFO,
            id = device_id.to_string(),
            name = device.name,
            "device exited recovery mode"
        );

        // Recovery mode is device-level: clear it only once no wallet still has
        // pending consolidations, so a device with several stays in recovery until
        // the last one finishes rather than going Ready after the first.
        let remaining = self
            .coordinator
            .lock()
            .unwrap()
            .pending_physical_consolidations(device_id);
        if remaining.is_empty() {
            self.device_list
                .lock()
                .unwrap()
                .set_recovery_mode(device_id, api::device_list::RecoveryMode::Off);

            self.ui_stack
                .lock()
                .unwrap()
                .connected(device_id, DeviceMode::Ready);
        }
    }

    pub fn delete_restoration_share(
        &self,
        restoration_id: RestorationId,
        device_id: DeviceId,
    ) -> Result<()> {
        {
            let mut coord = self.coordinator.lock().unwrap();
            let mut db = self.db.lock().unwrap();
            coord.staged_mutate(&mut *db, |coord| {
                coord.delete_restoration_share(restoration_id, device_id);
                Ok(())
            })?;
        }
        self.emit_key_state();
        Ok(())
    }

    pub fn emit_key_state(&self) {
        let coord = self.coordinator.lock().unwrap();
        let state = key_state(&coord);
        if let Some(key_event_stream) = &*self.key_event_stream.lock().unwrap() {
            key_event_stream.send(state);
        }
    }

    // Backup management methods
    pub(crate) fn build_backup_run(&self, key_id: KeyId) -> BackupRun {
        let backup_state = self.backup_state.lock().unwrap();
        let coordinator = self.coordinator.lock().unwrap();
        backup_run(&backup_state, &coordinator, key_id)
    }

    pub(crate) fn backup_stream_emit(&self, key_id: KeyId) -> Result<()> {
        let backup_run = self.build_backup_run(key_id);
        self.backup_run_streams
            .lock()
            .unwrap()
            .get(&key_id)
            .ok_or(anyhow!("no backup stream found for key: {}", key_id))?
            .add(backup_run)
            .unwrap();
        Ok(())
    }
}

/// The card format a restore's `EnterCard` names for a key: this build's format for a key the
/// app records as backed up on cards, else none (spec PK.8, build plan A8: the device then
/// never reports `NeedsNewerFirmware`).
fn card_recorded_format(format: BackupFormat) -> Option<u8> {
    (format == BackupFormat::Card64).then_some(CARD_FORMAT)
}

/// A wallet's share indices as a card names them, in a byte each. An index that doesn't fit is
/// left out: no card can hold it.
fn card_share_indices(indices: impl IntoIterator<Item = ShareIndex>) -> Vec<u8> {
    let mut indices: Vec<u8> = indices
        .into_iter()
        .filter_map(|index| u8::try_from(index).ok())
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// The share indices a card restore into a wallet filters repairs with: the wallet's own, plus
/// those its backup run started with (`issued`), which keep a deleted device's index. An issued
/// index of 0, or one no card can hold, is left out.
fn wallet_card_share_indices(
    wallet: impl IntoIterator<Item = ShareIndex>,
    issued: impl IntoIterator<Item = u32>,
) -> Vec<u8> {
    card_share_indices(
        wallet.into_iter().chain(
            issued
                .into_iter()
                .filter_map(|index| ShareIndex::try_from(index).ok()),
        ),
    )
}

impl FfiCoordinator {
    /// Starts `device_id`'s card flow. The device must be plugged in with the app's bundled
    /// firmware (design §6.6), except after N: a flow then starts without passing the gate, for
    /// the question an interruption asks, which needs no device (owner B9), and its `Begin`
    /// waits for the gate at the device's next connection, in the loop. What `card_begin`
    /// staged (an app closed mid-check is an interruption) is persisted before anything is
    /// sent.
    pub fn start_card_backup(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        sink: impl Sink<CardFlowState>,
    ) -> Result<()> {
        // Decided before anything is staged: a refused start leaves the record as it was.
        let gate = self.check_card_firmware(device_id);
        if gate.is_err() && !self.card_check_started_for(device_id, access_structure_ref) {
            return gate;
        }
        let mut protocol = {
            let mut coordinator = self.coordinator.lock().unwrap();
            let mut db = self.db.lock().unwrap();
            CardBackupProtocol::start(
                &mut coordinator,
                &mut *db,
                device_id,
                access_structure_ref,
                encryption_key,
                &mut rand::thread_rng(),
                sink,
            )?
        };
        // Locked as in `start_protocol`, and held from the cancel to the push.
        let mut ui_stack = self.ui_stack.lock().unwrap();
        // One card flow per device: a second start replaces the first.
        self.cancel_card_backup_in(&mut ui_stack, device_id);
        // `start` closed the device's restore entry, so a card restore on it would wait for
        // frames that no longer reach it: it ends with the entry.
        if ui_stack
            .cancel_where::<EnterPhysicalBackup>(|restore| restore.card_device() == Some(device_id))
        {
            self.usb_sender.send_cancel_all();
        }
        // The gate is checked again on the device as it is now, so a device that registered
        // since the first check never gets a `Begin` unchecked: the loop's gate ran before this
        // flow was on the stack. Connected with the bundled firmware, `connected` emits the
        // first state and sends the `Begin` (after the interruption's question, if one is up).
        // With other firmware (only after N) the flow waits on its own page, sending nothing. A
        // device unplugged since a passed gate ends it paused. A flow started after N without its
        // device waits for it.
        match self.device_list.lock().unwrap().get_device(device_id) {
            Some(device)
                if check_card_firmware(&device.firmware, device.latest_firmware.as_ref())
                    .is_ok() =>
            {
                protocol.connected(device_id, device.device_mode())
            }
            Some(_) => protocol.firmware_refused(device_id),
            None if gate.is_ok() => protocol.disconnected(device_id),
            None => protocol.emit_state(),
        }
        ui_stack.push(protocol);
        Ok(())
    }

    /// Whether `device_id`'s share of `access_structure_ref` has a check started (N recorded)
    /// and not ended, from the phone's record alone: its flow opens without the device.
    pub fn card_check_started_for(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
    ) -> bool {
        let coordinator = self.coordinator.lock().unwrap();
        coordinator
            .get_access_structure(access_structure_ref)
            .and_then(|access_structure| {
                access_structure
                    .device_to_share_indicies()
                    .get(&device_id)
                    .copied()
            })
            .is_some_and(|share_index| {
                coordinator.card_check_started_for(access_structure_ref, share_index)
            })
    }

    /// The user left `device_id`'s card flow: a pause, as an unplug is (design addendum §6).
    /// Unlike `cancel_protocol` it leaves every other protocol alone. A live flow's device gets
    /// a `Cancel` of its own, so it ends its session and goes home rather than run on with no
    /// page: a writing swipe would record a box the user never saw, and a device round that
    /// passed would take N and wait at a gate no phone serves. Core forgets the page and the
    /// gate screen as on an unplug, and keeps a verified question for the next flow. Leaving a
    /// running check after N is an interruption (owner B9, B12; review F1): core stages it, it's
    /// persisted here, and the flow opened next asks whether the user caused it.
    pub fn cancel_card_backup(&self, device_id: DeviceId) {
        let ended = self.cancel_card_backup_in(&mut self.ui_stack.lock().unwrap(), device_id);
        if ended {
            let mut coordinator = self.coordinator.lock().unwrap();
            let mut db = self.db.lock().unwrap();
            let left = persist_card_sends(&mut coordinator, &mut *db, |coordinator| {
                coordinator.card_flow_left(device_id);
                Ok::<_, CardActionError>(vec![])
            });
            if let Err(error) = left {
                event!(
                    Level::ERROR,
                    error = format!("{error:?}"),
                    "failed to persist leaving the card flow"
                );
            }
        }
    }

    /// Ends `device_id`'s card flow, and says whether it was live. A live flow's device gets a
    /// `Cancel`, sent before the stack is let go, so a card flow started next can't send its
    /// `Begin` ahead of it.
    fn cancel_card_backup_in(&self, ui_stack: &mut UiStack, device_id: DeviceId) -> bool {
        let live = leave_card_flow(ui_stack, device_id);
        if live {
            self.usb_sender.send_cancel(device_id);
        }
        live
    }

    /// The device is plugged in with the app's bundled firmware: the only firmware a card flow
    /// or card restore runs with (design §6.6).
    fn check_card_firmware(&self, device_id: DeviceId) -> Result<()> {
        let device_list = self.device_list.lock().unwrap();
        let device = device_list
            .get_device(device_id)
            .ok_or(anyhow!("Plug in this device first"))?;
        check_card_firmware(&device.firmware, device.latest_firmware.as_ref())
    }

    /// Next on a page of words 33–64. The flow turns it into the page core shows next, or
    /// moves to the confirm page by itself after the page of all the words. The words stay in
    /// RAM, so this works with the device unplugged (design §3.7).
    pub fn card_phone_next(&self, device_id: DeviceId) -> Result<()> {
        let page = self.card_flow(device_id, CardBackupProtocol::phone_next)?;
        match page {
            Some(page) => self.card_page_action(device_id, |coordinator| {
                coordinator.card_phone_page(device_id, page)
            }),
            None => Ok(()),
        }
    }

    /// Back on a page of words 33–64 or the confirm page; nothing on the first page.
    pub fn card_phone_back(&self, device_id: DeviceId) -> Result<()> {
        let page = self.card_flow(device_id, |card| card.phone_back())?;
        match page {
            Some(page) => self.card_page_action(device_id, |coordinator| {
                coordinator.card_phone_page(device_id, page)
            }),
            None => Ok(()),
        }
    }

    /// Runs `f` on `device_id`'s card flow, under the stack's lock alone.
    fn card_flow<T>(
        &self,
        device_id: DeviceId,
        f: impl FnOnce(&mut CardBackupProtocol) -> T,
    ) -> Result<T> {
        let mut ui_stack = self.ui_stack.lock().unwrap();
        let card = ui_stack
            .get_mut::<CardBackupProtocol>()
            .filter(|card| card.device_id() == device_id)
            .ok_or(anyhow!("this device's backup card isn't open"))?;
        Ok(f(card))
    }

    /// The hold on "Check you've written: words 33–64": the phone's quiz starts.
    pub fn card_phone_half_written(&self, device_id: DeviceId) -> Result<()> {
        self.card_page_action(device_id, |coordinator| {
            coordinator.card_phone_half_written(device_id)
        })
    }

    /// One pick in the phone's quiz, as a word index. Says whether it was right, so the page
    /// can show it as the device's quiz does: core's answer to a wrong pick is the same item
    /// again, marked `wrong`.
    pub fn card_self_check_answer(&self, device_id: DeviceId, pick: u16) -> Result<bool> {
        let mut wrong = false;
        self.card_action(device_id, |coordinator| {
            let sends = coordinator.card_self_check_answer(device_id, pick)?;
            wrong = sends.iter().any(|send| {
                matches!(
                    send,
                    CoordinatorSend::ToUser(CoordinatorToUserMessage::Card(CardToUser {
                        event: CardEvent::SelfCheckQuestion { wrong: true, .. },
                        ..
                    }))
                )
            });
            Ok(sends)
        })?;
        Ok(!wrong)
    }

    fn card_page_action(
        &self,
        device_id: DeviceId,
        action: impl FnOnce(&mut FrostCoordinator) -> Result<Vec<CoordinatorSend>, CardActionError>,
    ) -> Result<()> {
        let result = self.card_action(device_id, action).map(|_| ());
        if let Err(error) = &result {
            if error.downcast_ref::<CardActionError>() == Some(&CardActionError::Stale) {
                event!(
                    Level::DEBUG,
                    "ignoring a card page action for a page no longer on screen"
                );
                return Ok(());
            }
        }
        result
    }

    /// The hold on "Have you written words 1–32?" (owner B1): the device starts its proofs, and
    /// the phone shows words 33–64. Again after a restart, it re-sends the confirm. Owner B8:
    /// N is drawn here (once, persisted) and sent right after, so the device computes the
    /// openings of D while words 33–64 are written.
    pub fn card_device_half_written(&self, device_id: DeviceId) -> Result<()> {
        self.card_page_action(device_id, |coordinator| {
            // N straight from the OS CSPRNG (spec L1.4).
            coordinator.card_device_half_written(device_id, &mut rand::rngs::OsRng)
        })
    }

    /// Owner B15: the choice, on an install that didn't create the wallet, to write the card
    /// unverified. Words 33–64 are pulled for the device's own seal, then `WriteStart`.
    pub fn card_write_unverified(&self, device_id: DeviceId) -> Result<()> {
        self.card_page_action(device_id, |coordinator| {
            coordinator.card_write_unverified(device_id)
        })
    }

    /// [Continue] to the final check: the device round, on the N sent after the confirm of words
    /// 1–32 (owner B8). Core takes it only with both halves recorded and every proof verified.
    pub fn card_final_begin(&self, device_id: DeviceId) -> Result<()> {
        self.card_action(device_id, |coordinator| {
            // N straight from the OS CSPRNG (spec L1.4).
            coordinator.card_final_begin(device_id, &mut rand::rngs::OsRng)
        })
        .map(|_| ())
    }

    pub fn card_answer(&self, device_id: DeviceId, answer: BoxAnswer) -> Result<()> {
        self.card_action(device_id, |coordinator| {
            coordinator.card_answer(device_id, answer)
        })
        .map(|_| ())
    }

    /// Owner B9: the user's answer to "did you cause the interruption?". Yes: the flow sends
    /// `Begin` again once the device is in, and the check resumes on the same N. No: the
    /// device is marked suspect and the check ends. It needs no device. Returns its verdict if
    /// no flow heard it, for the page to show.
    pub fn card_interruption_answer(
        &self,
        device_id: DeviceId,
        caused_by_user: bool,
    ) -> Result<Option<CardFlowState>> {
        let result = self.card_action(device_id, |coordinator| {
            coordinator.card_interruption_answer(device_id, caused_by_user)
        });
        if let Err(error) = &result {
            if error.downcast_ref::<CardActionError>() == Some(&CardActionError::Stale) {
                event!(
                    Level::DEBUG,
                    "ignoring an answer to a question no longer up"
                );
                return Ok(None);
            }
        }
        result
    }

    /// Runs a card action. What it sends goes out only once its mutations are persisted; on a
    /// persist failure nothing is sent and card handling stops until restart (design §2.1 item
    /// 5). Locks in the loop's order: coordinator, db, ui_stack. Returns the action's verdict if
    /// no flow heard it ([`unheard_verdict`]).
    fn card_action(
        &self,
        device_id: DeviceId,
        action: impl FnOnce(&mut FrostCoordinator) -> Result<Vec<CoordinatorSend>, CardActionError>,
    ) -> Result<Option<CardFlowState>> {
        let mut coordinator = self.coordinator.lock().unwrap();
        let mut db = self.db.lock().unwrap();
        let mut ui_stack = self.ui_stack.lock().unwrap();
        let result = persist_card_sends(&mut coordinator, &mut *db, action);
        // Nothing below needs them, and a sink callback may.
        drop(db);
        drop(coordinator);
        let mut card = ui_stack
            .get_mut::<CardBackupProtocol>()
            .filter(|card| card.device_id() == device_id);
        match result {
            Ok(mut sends) => {
                // A `Begin` the loop hasn't sent yet goes first, sent under the stack's lock
                // as the loop's poll is, so it goes once and ahead of the request.
                if let Some(card) = card.as_mut() {
                    card.action_sent(&mut sends);
                }
                let mut unheard = None;
                for send in sends {
                    match send {
                        CoordinatorSend::ToUser(message) => {
                            if !ui_stack.process_to_user_message(message.clone()) {
                                let connected = self
                                    .device_list
                                    .lock()
                                    .unwrap()
                                    .get_device(device_id)
                                    .is_some();
                                unheard = unheard.or(unheard_verdict(&message, connected));
                            }
                        }
                        to_device => self.usb_sender.send_from_core([to_device]),
                    }
                }
                Ok(unheard)
            }
            Err(CardPersistError::Handler(error)) => Err(error.into()),
            Err(CardPersistError::Storage {
                error,
                storage_error,
            }) => {
                match (storage_error, card) {
                    (Some(message), _) => {
                        ui_stack.process_to_user_message(*message);
                    }
                    (None, Some(card)) => card.storage_error(),
                    (None, None) => {}
                }
                Err(error.context("couldn't save the card check; restart the app to continue"))
            }
        }
    }

    pub fn key_uses_card(&self, key_id: KeyId) -> bool {
        self.coordinator.lock().unwrap().key_backup_format(key_id) == BackupFormat::Card64
    }

    /// Every device is connected and its firmware has the card flow.
    pub fn devices_support_card(&self, devices: &[DeviceId]) -> bool {
        let device_list = self.device_list.lock().unwrap();
        devices.iter().all(|&id| {
            device_list
                .get_device(id)
                .is_some_and(|device| device.firmware.features().verifiable_card)
        })
    }
}

fn key_state(coordinator: &FrostCoordinator) -> api::coordinator::KeyState {
    let keys = coordinator
        .iter_keys()
        .cloned()
        .map(api::coordinator::FrostKey)
        .collect();

    let restoring = coordinator.restoring().collect();

    api::coordinator::KeyState { keys, restoring }
}

/// The backup checklist's run for a key, from the coordinator's record and the app's.
fn backup_run(
    backup_state: &BackupState,
    coordinator: &FrostCoordinator,
    key_id: KeyId,
) -> BackupRun {
    let frost_key = match coordinator.get_frost_key(key_id) {
        Some(key) => key,
        None => return BackupRun::default(),
    };

    let access_structure = frost_key
        .complete_key
        .access_structures
        .values()
        .next()
        .expect("access structure must exist");

    let backup_complete_states = backup_state.get_backup_run(key_id);
    let access_structure_ref = access_structure.access_structure_ref();

    let devices = access_structure
        .share_index_to_devices()
        .into_iter()
        .flat_map(|(share_index, device_ids)| {
            let share_index_short =
                u32::try_from(share_index).expect("share index should fit in u32");
            let card_status: Option<api::verifiable_card::CardBackupStatus> = coordinator
                .card_backup_status(access_structure_ref, share_index)
                .map(Into::into);
            // A card share is complete once both halves are recorded on this phone, which the
            // card page marks; PASSED and FAILED come from `card_status` (design §3.10).
            let complete = backup_complete_states.get(&share_index_short).copied();
            let failure = coordinator.card_failure(access_structure_ref, share_index);
            device_ids.into_iter().map(move |device_id| BackupDevice {
                device_id,
                share_index: share_index_short,
                complete,
                card_status,
                card_next_step: failure.and_then(|(reason, after_n)| {
                    frostsnap_coordinator::card_backup::failed_next_step(
                        reason,
                        coordinator.device_is_suspect(device_id),
                        after_n,
                    )
                }),
            })
        })
        .collect();

    BackupRun { devices }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn a_restore_names_the_card_format_only_for_a_card_key() {
        assert_eq!(card_recorded_format(BackupFormat::Card64), Some(3));
        assert_eq!(
            card_recorded_format(BackupFormat::Card64),
            Some(CARD_FORMAT)
        );
        assert_eq!(card_recorded_format(BackupFormat::Words25), None);
    }

    #[test]
    fn share_indices_go_as_bytes_sorted_without_repeats() {
        let index = |i: u32| ShareIndex::try_from(i).unwrap();
        assert_eq!(
            card_share_indices([index(3), index(1), index(2), index(1)]),
            [1, 2, 3]
        );
        // An index no card can hold is left out.
        assert_eq!(card_share_indices([index(255), index(256)]), [255]);
        assert!(card_share_indices([]).is_empty());
    }

    #[test]
    fn a_deleted_share_stays_in_the_wallet_set() {
        let index = |i: u32| ShareIndex::try_from(i).unwrap();
        // Share 2 was deleted from the wallet; the backup run still lists it.
        assert_eq!(
            wallet_card_share_indices([index(1), index(3)], [1, 2, 3]),
            [1, 2, 3]
        );
        // Index 0 and an index no card can hold are dropped.
        assert_eq!(
            wallet_card_share_indices([index(1)], [0, 2, 255, 256, 300]),
            [1, 2, 255]
        );
        // A key with no backup run (restored onto the phone) keeps the wallet's set.
        assert_eq!(wallet_card_share_indices([index(2), index(1)], []), [1, 2]);
    }
}
