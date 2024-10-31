use crate::api::{self, KeyState};
use crate::sink_wrap::SinkWrap;
use crate::TEMP_KEY;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::{RustOpaque, StreamSink};
use frostsnap_coordinator::check_share::CheckShareState;
use frostsnap_coordinator::firmware_upgrade::{
    FirmwareUpgradeConfirmState, FirmwareUpgradeProtocol,
};
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination, FirmwareDigest,
};
use frostsnap_coordinator::frostsnap_core::coordinator::{
    AccessStructureRef, CoordAccessStructure, CoordFrostKey,
};
use frostsnap_coordinator::frostsnap_core::message::CoordinatorSend;
use frostsnap_coordinator::frostsnap_core::SymmetricKey;
use frostsnap_coordinator::frostsnap_persist::DeviceNames;
use frostsnap_coordinator::persist::Persisted;
use frostsnap_coordinator::{
    check_share::CheckShareProtocol, display_backup::DisplayBackupProtocol,
};
use frostsnap_coordinator::{
    frostsnap_core, AppMessageBody, FirmwareBin, UiProtocol, UsbSender, UsbSerialManager,
};
use frostsnap_coordinator::{Completion, DeviceChange};
use frostsnap_core::{
    coordinator::{FrostCoordinator, SigningSessionState},
    DeviceId, KeyId, SignTask,
};
use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{event, Level};

pub struct FfiCoordinator {
    usb_manager: Mutex<Option<UsbSerialManager>>,
    pending_for_outbox: Arc<Mutex<VecDeque<CoordinatorSend>>>,
    key_event_stream: Arc<Mutex<Option<StreamSink<KeyState>>>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    ui_protocol: Arc<Mutex<Option<Box<dyn UiProtocol>>>>,
    usb_sender: UsbSender,
    firmware_bin: FirmwareBin,
    firmware_upgrade_progress: Arc<Mutex<Option<StreamSink<f32>>>>,

    // persisted things
    db: Arc<Mutex<rusqlite::Connection>>,
    device_names: Arc<Mutex<Persisted<DeviceNames>>>,
    coordinator: Arc<Mutex<Persisted<FrostCoordinator>>>,
    signing_session: Arc<Mutex<Persisted<Option<SigningSessionState>>>>,
}

impl FfiCoordinator {
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        usb_manager: UsbSerialManager,
    ) -> anyhow::Result<Self> {
        let pending_for_outbox = Arc::new(Mutex::new(VecDeque::new()));

        let mut db_ = db.lock().unwrap();

        event!(Level::DEBUG, "loading core coordinator");
        let coordinator = Persisted::<FrostCoordinator>::new(&mut db_, ())?;
        event!(Level::DEBUG, "loading device names");
        let device_names = Persisted::<DeviceNames>::new(&mut db_, ())?;
        event!(Level::DEBUG, "loading saved signing session");
        let signing_session = Persisted::<Option<SigningSessionState>>::new(&mut db_, ())?;

        let usb_sender = usb_manager.usb_sender();
        let firmware_bin = usb_manager.upgrade_bin();

        let usb_manager = Mutex::new(Some(usb_manager));
        drop(db_);

        Ok(Self {
            usb_manager,
            pending_for_outbox,
            thread_handle: Default::default(),
            key_event_stream: Default::default(),
            ui_protocol: Default::default(),
            firmware_upgrade_progress: Default::default(),
            usb_sender,
            firmware_bin,
            db,
            coordinator: Arc::new(Mutex::new(coordinator)),
            device_names: Arc::new(Mutex::new(device_names)),
            signing_session: Arc::new(Mutex::new(signing_session)),
        })
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
        let pending_loop = self.pending_for_outbox.clone();
        let coordinator_loop = self.coordinator.clone();
        let ui_protocol = self.ui_protocol.clone();
        let db_loop = self.db.clone();
        let device_names = self.device_names.clone();
        let usb_sender = self.usb_sender.clone();
        let firmware_upgrade_progress = self.firmware_upgrade_progress.clone();
        let signing_session = self.signing_session.clone();

        let handle = std::thread::spawn(move || {
            loop {
                // to give time for the other threads to get a lock
                std::thread::sleep(Duration::from_millis(100));

                // check for firmware upgrade mode before locking anything else
                let mut firmware_upgrade_progress_loop = firmware_upgrade_progress.lock().unwrap();
                if let Some(firmware_upgrade_pogress) = &mut *firmware_upgrade_progress_loop {
                    // We're in a firmware upgrade.
                    // Do the firmware upgrade and then carry on as usual
                    let progress_iter = usb_manager.run_firmware_upgrade();
                    let mut error = Ok(());
                    for progress in progress_iter {
                        match progress {
                            Ok(progress) => {
                                firmware_upgrade_pogress.add(progress);
                            }
                            Err(e) => {
                                error = Err(e);
                                break;
                            }
                        }
                    }

                    firmware_upgrade_pogress.close();
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
                let mut ui_protocol_loop = ui_protocol.lock().unwrap();
                let mut coordinator_outbox = pending_loop.lock().unwrap();
                let mut messages_from_devices = vec![];
                let mut db = db_loop.lock().unwrap();

                // process new messages from devices
                {
                    for change in &device_changes {
                        match change {
                            DeviceChange::Registered { id, .. } => {
                                if let Some(protocol) = &mut *ui_protocol_loop {
                                    protocol.connected(*id);
                                }
                                coordinator_outbox
                                    .extend(coordinator.maybe_request_nonce_replenishment(*id));
                            }
                            DeviceChange::Disconnected { id } => {
                                if let Some(protocol) = &mut *ui_protocol_loop {
                                    protocol.disconnected(*id);
                                }
                            }
                            DeviceChange::NameChange { id, name } => {
                                let mut device_names = device_names.lock().unwrap();
                                // TODO: Detect name change and prompt user to accept
                                let result = device_names.staged_mutate(&mut *db, |names| {
                                    names.insert(*id, name.clone());
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
                                        usb_manager.accept_device_name(*id, name.clone());
                                    }
                                }
                            }
                            DeviceChange::AppMessage(message) => {
                                messages_from_devices.push(message.clone());
                            }
                            _ => { /* ignore rest */ }
                        }
                    }

                    if let Some(ui_protocol) = &mut *ui_protocol_loop {
                        let (to_device, to_storage) = ui_protocol.poll();
                        for message in to_device {
                            usb_sender.send(message);
                        }

                        for message in to_storage {
                            match message {
                                frostsnap_coordinator::UiToStorageMessage::ClearSigningSession => {
                                    let result = signing_session.lock().unwrap().mutate(
                                        &mut *db,
                                        |session| {
                                            *session = None;
                                            Ok(((), session.clone()))
                                        },
                                    );

                                    if let Err(e) = result {
                                        event!(
                                            Level::ERROR,
                                            error = e.to_string(),
                                            "failed to clear signing session on disk"
                                        );
                                    }
                                }
                            }
                        }

                        Self::try_finish_protocol(
                            usb_sender.clone(),
                            coordinator.MUTATE_NO_PERSIST(),
                            &mut ui_protocol_loop,
                        );
                    }

                    crate::api::emit_device_events(
                        device_changes
                            .into_iter()
                            .map(crate::api::DeviceChange::from)
                            .collect(),
                    );
                };

                for app_message in messages_from_devices {
                    match app_message.body {
                        AppMessageBody::Core(core_message) => {
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
                        AppMessageBody::AckUpgradeMode => {
                            if let Some(ui_protocol) = &mut *ui_protocol_loop {
                                ui_protocol.process_upgrade_mode_ack(app_message.from);
                            }
                        }
                    }
                }

                drop(coordinator);

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
                            if let Some(ui_protocol) = &mut *ui_protocol_loop {
                                ui_protocol.process_to_user_message(msg);
                            }
                        }
                        CoordinatorSend::SigningSessionStore(state) => {
                            let result = signing_session.lock().unwrap().staged_mutate(
                                &mut *db,
                                |signing_session| {
                                    *signing_session = Some(state);
                                    Ok(())
                                },
                            );

                            if let Err(e) = result {
                                event!(
                                    Level::ERROR,
                                    error = e.to_string(),
                                    "failed to sign session progress"
                                );
                            }
                        }
                    }
                }
            }
        });

        *self.thread_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn sub_key_events(&self, stream: StreamSink<crate::api::KeyState>) {
        stream.add(crate::api::KeyState {
            keys: self
                .frost_keys()
                .into_iter()
                .map(crate::api::FrostKey::from)
                .collect(),
        });
        let mut key_event_stream = self.key_event_stream.lock().unwrap();
        if let Some(existing) = key_event_stream.replace(stream) {
            existing.close();
        }
    }

    pub fn update_name_preview(&self, id: DeviceId, name: &str) {
        self.usb_sender.update_name_preview(id, name);
    }

    pub fn finish_naming(&self, id: DeviceId, name: &str) {
        self.usb_sender.finish_naming(id, name);
    }

    pub fn send_cancel(&self, id: DeviceId) {
        self.usb_sender.send_cancel(id)
    }

    pub fn generate_new_key(
        &self,
        devices: BTreeSet<DeviceId>,
        threshold: u16,
        key_name: String,
        sink: StreamSink<frostsnap_coordinator::keygen::KeyGenState>,
    ) -> anyhow::Result<()> {
        let currently_connected = api::device_list_state()
            .0
            .devices
            .into_iter()
            .map(|device| device.id)
            .collect();
        let ui_protocol = frostsnap_coordinator::keygen::KeyGen::new(
            SinkWrap(sink),
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            devices.clone(),
            currently_connected,
            threshold,
            key_name,
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

    pub fn nonces_left(&self, id: DeviceId) -> Option<usize> {
        self.coordinator
            .lock()
            .unwrap()
            .device_nonces()
            .get(&id)
            .map(|nonces| nonces.nonces.len())
    }

    pub fn current_nonce(&self, id: DeviceId) -> Option<u64> {
        self.coordinator
            .lock()
            .unwrap()
            .device_nonces()
            .get(&id)
            .map(|nonces| nonces.start_index)
    }

    pub fn start_signing(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: BTreeSet<DeviceId>,
        task: SignTask,
        sink: StreamSink<api::SigningState>,
        encryption_key: SymmetricKey,
    ) -> anyhow::Result<()> {
        let mut coordinator = self.coordinator.lock().unwrap();
        let mut messages =
            coordinator.staged_mutate(&mut self.db.lock().unwrap(), |coordinator| {
                Ok(coordinator.start_sign(
                    access_structure_ref,
                    task,
                    devices.clone(),
                    encryption_key,
                )?)
            })?;
        let mut ui_protocol =
            frostsnap_coordinator::signing::SigningDispatcher::from_filter_out_start_sign(
                &mut messages,
                SinkWrap(sink),
            );

        self.pending_for_outbox.lock().unwrap().extend(messages);
        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn try_restore_signing_session(
        &self,
        #[allow(unused)] /* we only have one key for now */ master_appkey: KeyId,
        sink: StreamSink<api::SigningState>,
    ) -> anyhow::Result<()> {
        let signing_session_state = self.signing_session.lock().unwrap();
        let signing_session_state = signing_session_state
            .clone()
            .ok_or(anyhow!("no signing session to restore"))?;
        let mut coordinator = self.coordinator.lock().unwrap();
        coordinator
            .MUTATE_NO_PERSIST()
            .restore_sign_session(signing_session_state.clone());

        let mut dispatcher = frostsnap_coordinator::signing::SigningDispatcher::new_from_request(
            signing_session_state.request.clone(),
            signing_session_state.targets.clone(),
            SinkWrap(sink),
        );

        for already_provided in signing_session_state.received_from() {
            dispatcher.set_signature_received(already_provided);
        }

        dispatcher.emit_state();
        self.start_protocol(dispatcher);

        Ok(())
    }

    pub fn persisted_sign_session_description(
        &self,
        key_id: KeyId,
    ) -> Option<api::SignTaskDescription> {
        let session = self.signing_session.lock().unwrap().clone()?;
        if session.access_structure.master_appkey().key_id() != key_id {
            return None;
        }
        Some(match session.request.sign_task {
            SignTask::Plain { message, .. } => api::SignTaskDescription::Plain { message },
            SignTask::Nostr { .. } => todo!("nostr restoring not yet implemented"),
            SignTask::BitcoinTransaction(task) => api::SignTaskDescription::Transaction {
                unsigned_tx: api::UnsignedTx {
                    template_tx: RustOpaque::new(task),
                },
            },
        })
    }

    pub fn request_display_backup(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        stream: StreamSink<bool>,
    ) -> anyhow::Result<()> {
        let backup_protocol = DisplayBackupProtocol::new(
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            device_id,
            access_structure_ref,
            encryption_key,
            SinkWrap(stream),
        )?;

        self.start_protocol(backup_protocol);

        Ok(())
    }

    pub fn begin_upgrade_firmware(
        &self,
        sink: StreamSink<FirmwareUpgradeConfirmState>,
    ) -> anyhow::Result<()> {
        let bin = self.firmware_bin;
        let devices = api::device_list_state()
            .0
            .devices
            .into_iter()
            .map(|device| device.id)
            .collect();

        let need_upgrade = api::device_list_state()
            .0
            .devices
            .into_iter()
            .filter(|device| device.needs_firmware_upgrade().0)
            .map(|device| device.id)
            .collect();

        let ui_protocol = FirmwareUpgradeProtocol::new(devices, need_upgrade, bin, SinkWrap(sink));
        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn upgrade_firmware_digest(&self) -> FirmwareDigest {
        self.firmware_bin.digest()
    }

    fn start_protocol<P: UiProtocol + Send + 'static>(&self, mut protocol: P) {
        event!(Level::INFO, "Starting UI protocol {}", protocol.name());
        for device in api::device_list_state().0.devices {
            protocol.connected(device.id);
        }
        let new_name = protocol.name();
        if let Some(mut prev) = self.ui_protocol.lock().unwrap().replace(Box::new(protocol)) {
            event!(
                Level::WARN,
                prev = prev.name(),
                new = new_name,
                "previous protocol wasn't shut down cleanly"
            );
            prev.cancel();
        }
    }

    pub fn cancel_protocol(&self) {
        let mut proto_opt = self.ui_protocol.lock().unwrap();
        if let Some(proto) = &mut *proto_opt {
            proto.cancel();
            assert!(
                Self::try_finish_protocol(
                    self.usb_sender.clone(),
                    self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
                    &mut proto_opt
                ),
                "protocol must be finished after cancel"
            );
        }
    }

    fn try_finish_protocol(
        usb_sender: UsbSender,
        coordinator: &mut FrostCoordinator,
        proto_opt: &mut Option<Box<dyn UiProtocol>>,
    ) -> bool {
        if let Some(proto) = proto_opt {
            if let Some(completion) = proto.is_complete() {
                event!(
                    Level::INFO,
                    "UI Protocol {} completed with {:?}",
                    proto.name(),
                    completion
                );
                match completion {
                    Completion::Abort {
                        send_cancel_to_all_devices,
                    } => {
                        if send_cancel_to_all_devices {
                            usb_sender.send_cancel_all();
                        }
                        coordinator.cancel();
                        *proto_opt = None;
                        return true;
                    }
                    Completion::Success => {
                        *proto_opt = None;
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn enter_firmware_upgrade_mode(&self, sink: StreamSink<f32>) -> Result<()> {
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
            progress => *progress = Some(sink),
        }
        Ok(())
    }

    pub fn get_device_name(&self, id: DeviceId) -> Option<String> {
        self.device_names.lock().unwrap().get(id)
    }

    pub fn final_keygen_ack(&self) -> Result<AccessStructureRef> {
        let mut coordinator = self.coordinator.lock().unwrap();
        let mut db = self.db.lock().unwrap();
        let accs_ref = coordinator.staged_mutate(&mut db, |coordinator| {
            Ok(coordinator.final_keygen_ack(TEMP_KEY, &mut rand::thread_rng())?)
        })?;

        let mut proto = self.ui_protocol.lock().unwrap();
        let keygen = proto
            .as_mut()
            .ok_or(anyhow!("No UI protocol running"))?
            .as_mut_any()
            .downcast_mut::<frostsnap_coordinator::keygen::KeyGen>()
            .ok_or(anyhow!("somehow UI was not in KeyGen state"))?;

        keygen.final_keygen_ack(accs_ref);

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.add(KeyState {
                keys: frost_keys(&coordinator),
            });
        }

        assert!(
            Self::try_finish_protocol(
                self.usb_sender.clone(),
                coordinator.MUTATE_NO_PERSIST(),
                &mut proto
            ),
            "keygen must be finished after we call final ack"
        );
        Ok(accs_ref)
    }

    pub fn get_access_structure(&self, as_ref: AccessStructureRef) -> Option<CoordAccessStructure> {
        self.coordinator
            .lock()
            .unwrap()
            .get_access_structure(as_ref)
    }

    pub fn check_share_on_device(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        stream: StreamSink<CheckShareState>,
    ) -> anyhow::Result<()> {
        let check_share_protocol = CheckShareProtocol::new(
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            device_id,
            access_structure_ref,
            SinkWrap(stream),
        );
        check_share_protocol.emit_state();
        self.start_protocol(check_share_protocol);
        Ok(())
    }

    pub fn get_frost_key(&self, key_id: KeyId) -> Option<CoordFrostKey> {
        self.coordinator
            .lock()
            .unwrap()
            .get_frost_key(key_id)
            .cloned()
    }
}

fn frost_keys(coordinator: &FrostCoordinator) -> Vec<crate::api::FrostKey> {
    coordinator
        .iter_keys()
        .map(|coord_frost_key| crate::api::FrostKey(RustOpaque::new(coord_frost_key.clone())))
        .collect()
}
