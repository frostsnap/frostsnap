#![allow(unused)]
use crate::api;
use crate::api::coordinator::KeyState;
use crate::api::device_list::DeviceListUpdate;
use crate::device_list::DeviceList;
use anyhow::{anyhow, Result};
use frostsnap_coordinator::display_backup::DisplayBackupProtocol;
use frostsnap_coordinator::enter_physical_backup::{EnterPhysicalBackup, EnterPhysicalBackupState};
use frostsnap_coordinator::firmware_upgrade::{
    FirmwareUpgradeConfirmState, FirmwareUpgradeProtocol,
};
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination, Sha256Digest,
};
use frostsnap_coordinator::frostsnap_persist::DeviceNames;
use frostsnap_coordinator::persist::Persisted;
use frostsnap_coordinator::signing::SigningState;
use frostsnap_coordinator::verify_address::{VerifyAddressProtocol, VerifyAddressProtocolState};
use frostsnap_coordinator::wait_for_recovery_share::{
    WaitForRecoveryShare, WaitForRecoveryShareState,
};
use frostsnap_coordinator::{
    AppMessageBody, DeviceChange, DeviceMode, FirmwareBin, Sink, UiProtocol, UiStack, UsbSender,
    UsbSerialManager, WaitForToUserMessage,
};
use frostsnap_core::coordinator::restoration::{
    PhysicalBackupPhase, RecoverShare, RestorationState, ToUserRestoration,
};
use frostsnap_core::coordinator::{
    CoordAccessStructure, CoordFrostKey, CoordinatorSend, CoordinatorToUserMessage,
    FrostCoordinator,
};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::{
    message, AccessStructureRef, DeviceId, KeyId, KeygenId, RestorationId, SignSessionId,
    SymmetricKey, WireSignTask,
};
use std::collections::{BTreeSet, VecDeque};
use std::ops::DerefMut;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};
const N_NONCE_STREAMS: usize = 4;

pub struct FfiCoordinator {
    usb_manager: Mutex<Option<UsbSerialManager>>,
    key_event_stream: Arc<Mutex<Option<Box<dyn Sink<KeyState>>>>>,
    signing_session_signals: Arc<Mutex<HashMap<KeyId, Signal>>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    ui_stack: Arc<Mutex<UiStack>>,
    pub(crate) usb_sender: UsbSender,
    firmware_bin: Option<FirmwareBin>,
    firmware_upgrade_progress: Arc<Mutex<Option<Box<dyn Sink<f32>>>>>,
    device_list: Arc<Mutex<DeviceList>>,
    device_list_stream: Arc<Mutex<Option<Box<dyn Sink<DeviceListUpdate>>>>>,
    // // persisted things
    db: Arc<Mutex<rusqlite::Connection>>,
    device_names: Arc<Mutex<Persisted<DeviceNames>>>,
    coordinator: Arc<Mutex<Persisted<FrostCoordinator>>>,
}

type Signal = Box<dyn Sink<()>>;

impl FfiCoordinator {
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        usb_manager: UsbSerialManager,
    ) -> anyhow::Result<Self> {
        let mut db_ = db.lock().unwrap();

        event!(Level::DEBUG, "loading core coordinator");
        let coordinator = Persisted::<FrostCoordinator>::new(&mut db_, ())?;
        event!(Level::DEBUG, "loading device names");
        let device_names = Persisted::<DeviceNames>::new(&mut db_, ())?;

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
                let mut db = db_loop.lock().unwrap();
                let mut ui_stack = ui_stack.lock().unwrap();

                // process new messages from devices
                {
                    let mut device_list = device_list.lock().unwrap();
                    for change in device_changes {
                        device_list.consume_manager_event(change.clone());
                        match change {
                            DeviceChange::Registered { id, .. } => {
                                if coordinator.has_backups_that_need_to_be_consolidated(id) {
                                    device_list.set_recovery_mode(id, true);
                                }

                                ui_stack.connected(
                                    id,
                                    device_list
                                        .get_device(id)
                                        .expect("it was just registered")
                                        .device_mode(),
                                );

                                if let Some(connected_device) = device_list.get_device(id) {
                                    // we only send some messages out if the device has up to date firmware
                                    if !connected_device.needs_firmware_upgrade() {
                                        coordinator_outbox.extend(
                                            coordinator.maybe_request_nonce_replenishment(
                                                id,
                                                N_NONCE_STREAMS,
                                                &mut rand::thread_rng(),
                                            ),
                                        );
                                    }
                                }
                            }
                            DeviceChange::Disconnected { id } => {
                                ui_stack.disconnected(id);
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
                            _ => { /* ignore rest */ }
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
                        AppMessageBody::Misc(comms_misc) => {
                            ui_stack.process_comms_message(app_message.from, comms_misc);
                        }
                    }
                }

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
        purpose: KeyPurpose,
        sink: impl Sink<frostsnap_coordinator::keygen::KeyGenState>,
    ) -> anyhow::Result<()> {
        let currently_connected = self
            .device_list
            .lock()
            .unwrap()
            .devices()
            .into_iter()
            .map(|device| device.id)
            .collect();

        let begin_keygen = message::keygen::Begin::new(
            devices,
            threshold,
            key_name,
            purpose,
            &mut rand::thread_rng(),
        );

        let ui_protocol = frostsnap_coordinator::keygen::KeyGen::new(
            sink,
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            currently_connected,
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

    pub fn request_display_backup(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        stream: impl Sink<bool>,
    ) -> anyhow::Result<()> {
        let backup_protocol = DisplayBackupProtocol::new(
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            device_id,
            access_structure_ref,
            encryption_key,
            stream,
        )?;

        self.start_protocol(backup_protocol);

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

            let devices = device_list
                .devices()
                .into_iter()
                .map(|device| device.id)
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

    fn start_protocol<P: UiProtocol + Send + 'static>(&self, mut protocol: P) {
        for device in self.device_list.lock().unwrap().devices() {
            protocol.connected(device.id, device.device_mode());
        }

        let mut stack = self.ui_stack.lock().unwrap();
        stack.push(protocol);
    }

    pub fn cancel_protocol(&self) {
        if self.ui_stack.lock().unwrap().cancel_all() {
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

    pub fn finalize_keygen(
        &self,
        keygen_id: KeygenId,
        symmetric_key: SymmetricKey,
    ) -> Result<AccessStructureRef> {
        let access_structure_ref = {
            let mut coordinator = self.coordinator.lock().unwrap();
            let mut db = self.db.lock().unwrap();
            let mut ui_stack = self.ui_stack.lock().unwrap();
            let keygen = ui_stack
                .get_mut::<frostsnap_coordinator::keygen::KeyGen>()
                .ok_or(anyhow!("somehow UI was not in KeyGen state"))?;

            let finalized_keygen = coordinator.staged_mutate(&mut db, |coordinator| {
                Ok(coordinator.finalize_keygen(
                    keygen_id,
                    symmetric_key,
                    &mut rand::thread_rng(),
                )?)
            })?;
            let access_structure_ref = finalized_keygen.access_structure_ref;

            self.usb_sender.send_from_core(finalized_keygen);
            keygen.keygen_finalized(access_structure_ref);
            access_structure_ref
        };

        self.emit_key_state();

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

        let verify_address_messages = coordinator.verify_address(key_id, address_index)?;

        let ui_protocol = VerifyAddressProtocol::new(verify_address_messages.clone(), stream);

        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn key_state(&self) -> api::coordinator::KeyState {
        key_state(&self.coordinator.lock().unwrap())
    }

    pub fn wait_for_recovery_share(&self, sink: impl Sink<WaitForRecoveryShareState>) {
        let ui_protocol = WaitForRecoveryShare::new(sink);
        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);
    }

    pub fn start_restoring_wallet(
        &self,
        name: String,
        threshold: u16,
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

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.send(self.key_state());
        }

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

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.send(self.key_state());
        }

        Ok(restoration_id)
    }

    pub fn continue_restoring_wallet_from_device_share(
        &self,
        restoration_id: RestorationId,
        mut recover_share: &RecoverShare,
    ) -> Result<()> {
        {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            let restoration_state = coordinator
                .get_restoration_state(restoration_id)
                .ok_or(anyhow!("non-existent restoration"))?;

            let mut held_share = recover_share.held_share.clone();

            // HACK: We overwrite the name of the device share here to contrive compatibility.
            if restoration_state.key_name != held_share.key_name {
                event!(
                    Level::WARN,
                    recovery_name = restoration_state.key_name,
                    device_name = held_share.key_name,
                    "had to rename restoration share"
                );
                held_share.key_name = restoration_state.key_name.clone();
            }
            coordinator.staged_mutate(&mut *db, |coordinator| {
                coordinator.add_recovery_share_to_restoration(restoration_id, recover_share)?;
                Ok(())
            })?;
        }

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.send(self.key_state());
        }
        Ok(())
    }

    pub fn recover_share(
        &self,
        access_structure_ref: AccessStructureRef,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        {
            let held_by = recover_share.held_by;
            {
                let mut db = self.db.lock().unwrap();
                let mut coordinator = self.coordinator.lock().unwrap();
                coordinator.staged_mutate(&mut *db, |coordinator| {
                    coordinator.recover_share(
                        access_structure_ref,
                        recover_share,
                        encryption_key,
                    )?;
                    Ok(())
                })?;
            }

            self.exit_recovery_mode(held_by, encryption_key);

            if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
                stream.send(self.key_state());
            }

            Ok(())
        }
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
            let needs_consolidation = restoration_state.need_to_consolidate;
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
            self.exit_recovery_mode(device_id, encryption_key);
        }

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.send(self.key_state());
        }

        Ok(assid)
    }

    pub fn delete_key(&self, key_id: KeyId) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let mut coordinator = self.coordinator.lock().unwrap();
        coordinator.staged_mutate(&mut *db, |coordinator| {
            coordinator.delete_key(key_id);
            Ok(())
        })?;
        drop(coordinator);

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.send(self.key_state());
        }

        Ok(())
    }

    pub fn sub_device_events(&self, new_stream: impl Sink<api::device_list::DeviceListUpdate>) {
        let mut device_list_stream = self.device_list_stream.lock().unwrap();
        let mut device_list = self.device_list.lock().unwrap();
        new_stream.send(device_list.take_update());
        device_list_stream.replace(Box::new(new_stream));
    }

    pub fn device_at_index(&self, index: usize) -> Option<api::device_list::ConnectedDevice> {
        self.device_list.lock().unwrap().device_at_index(index)
    }

    pub fn device_list_state(&self) -> api::device_list::DeviceListState {
        self.device_list.lock().unwrap().take_update().state
    }

    pub fn get_connected_device(&self, id: DeviceId) -> Option<api::device_list::ConnectedDevice> {
        self.device_list.lock().unwrap().get_device(id)
    }

    pub fn wipe_device_data(&self, id: DeviceId) {
        self.usb_sender.wipe_device_data(id);
    }

    pub fn wipe_all_devices(&self) {
        self.usb_sender.wipe_all()
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
        let device_list = self.device_list.clone();
        let coordinator = self.coordinator.clone();
        let key_event_stream = self.key_event_stream.clone();
        let sink = sink.inspect(move |state| {
            if state.saved {
                // When/if the physical backup has been saved on the device this means the device
                // has entered recovery mode. We need to set recovery mode so that the device can be
                // brought out of recovery mode when the time comes.
                device_list
                    .lock()
                    .unwrap()
                    .set_recovery_mode(device_id, true);

                let coordinator = coordinator.lock().unwrap();
                // We need to update the recovering key's state the ui gets updated
                let state = key_state(&coordinator);
                if let Some(key_event_stream) = &*key_event_stream.lock().unwrap() {
                    key_event_stream.send(state);
                }
            }
        });
        let proto = EnterPhysicalBackup::new(sink, device_id);
        self.start_protocol(proto);
        Ok(())
    }

    // XXX: Cannot be called during another UI protocol
    pub fn tell_device_to_save_physical_backup(
        &self,
        phase: PhysicalBackupPhase,
        restoration_id: RestorationId,
    ) {
        {
            let mut coord = self.coordinator.lock().unwrap();
            let messages = coord
                .MUTATE_NO_PERSIST()
                .tell_device_to_save_physical_backup(phase, restoration_id);
            self.usb_sender.send_from_core(messages);
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
                    && share_index == phase.backup.share_image.share_index;
            }
            false
        });
        if success {
            self.device_list
                .lock()
                .unwrap()
                .set_recovery_mode(phase.from, true);

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
                    && share_index == phase.backup.share_image.share_index;
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
    pub fn exit_recovery_mode(&self, device_id: DeviceId, encryption_key: SymmetricKey) {
        let device = match self.device_list.lock().unwrap().get_device(device_id) {
            Some(device) => device,
            None => return,
        };

        let msgs = {
            let coord = self.coordinator.lock().unwrap();
            coord
                .consolidate_pending_physical_backups(device_id, encryption_key)
                .into_iter()
                .collect::<Vec<_>>()
        };

        if msgs.is_empty() {
            return;
        }

        self.usb_sender.send_from_core(msgs);

        event!(
            Level::INFO,
            id = device_id.to_string(),
            name = device.name,
            "asking device to exit recovery mode"
        );

        let success = self.block_for_to_user_message([device_id], move |to_user| match to_user {
            CoordinatorToUserMessage::Restoration(ToUserRestoration::FinishedConsolidation {
                device_id: got,
                ..
            }) => device_id == got,
            _ => false,
        });

        if success {
            event!(
                Level::INFO,
                id = device_id.to_string(),
                name = device.name,
                "device exited recovery mode"
            );

            self.device_list
                .lock()
                .unwrap()
                .set_recovery_mode(device_id, false);

            self.ui_stack
                .lock()
                .unwrap()
                .connected(device_id, DeviceMode::Ready);
        } else {
            event!(
                Level::ERROR,
                id = device_id.to_string(),
                name = device.name,
                "device failed to exit recovery mode"
            );
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
}

fn key_state(coordinator: &FrostCoordinator) -> api::coordinator::KeyState {
    let keys = coordinator
        .iter_keys()
        .cloned()
        .map(api::coordinator::FrostKey)
        .collect();

    let restoring = coordinator
        .restoring()
        .map(|restoring| {
            let status = restoring.status();
            api::recovery::RestoringKey {
                problem: status.problem(),
                shares_obtained: status.shares,
                restoration_id: restoring.restoration_id,
                name: restoring.key_name,
                threshold: restoring.access_structure.threshold,
                bitcoin_network: restoring.key_purpose.bitcoin_network(),
            }
        })
        .collect();

    api::coordinator::KeyState { keys, restoring }
}
