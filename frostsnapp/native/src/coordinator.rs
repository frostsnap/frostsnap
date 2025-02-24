use crate::api;
use crate::device_list::DeviceList;
use crate::sink_wrap::SinkWrap;
use crate::TEMP_KEY;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::{RustOpaque, StreamSink, SyncReturn};
use frostsnap_coordinator::check_share::CheckShareState;
use frostsnap_coordinator::firmware_upgrade::{
    FirmwareUpgradeConfirmState, FirmwareUpgradeProtocol,
};
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination, Sha256Digest,
};
use frostsnap_coordinator::frostsnap_core::coordinator::{
    CoordAccessStructure, CoordFrostKey, CoordinatorSend,
};
use frostsnap_coordinator::frostsnap_core::device::KeyPurpose;
use frostsnap_coordinator::frostsnap_core::message::{
    CoordinatorToUserMessage, DoKeyGen, RecoverShare,
};
use frostsnap_coordinator::frostsnap_core::SymmetricKey;
use frostsnap_coordinator::frostsnap_core::{self, SignSessionId};
use frostsnap_coordinator::frostsnap_persist::DeviceNames;
use frostsnap_coordinator::persist::Persisted;
use frostsnap_coordinator::verify_address::VerifyAddressProtocol;
use frostsnap_coordinator::{
    check_share::CheckShareProtocol, display_backup::DisplayBackupProtocol,
};
use frostsnap_coordinator::{AppMessageBody, FirmwareBin, UiProtocol, UsbSender, UsbSerialManager};
use frostsnap_coordinator::{Completion, DeviceChange};
use frostsnap_core::{
    coordinator::FrostCoordinator, AccessStructureRef, DeviceId, KeyId, SignTask,
};
use rand::thread_rng;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{event, span, Level};

pub struct FfiCoordinator {
    usb_manager: Mutex<Option<UsbSerialManager>>,
    key_event_stream: Arc<Mutex<Option<StreamSink<api::KeyState>>>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    ui_protocol: Arc<Mutex<Option<Box<dyn UiProtocol>>>>,
    usb_sender: UsbSender,
    firmware_bin: Option<FirmwareBin>,
    firmware_upgrade_progress: Arc<Mutex<Option<StreamSink<f32>>>>,
    recoverable_keys: Arc<Mutex<BTreeMap<AccessStructureRef, Vec<RecoverShare>>>>,

    device_list: Arc<Mutex<DeviceList>>,
    device_list_stream: Arc<Mutex<Option<StreamSink<api::DeviceListUpdate>>>>,

    // persisted things
    db: Arc<Mutex<rusqlite::Connection>>,
    device_names: Arc<Mutex<Persisted<DeviceNames>>>,
    coordinator: Arc<Mutex<Persisted<FrostCoordinator>>>,
}

impl FfiCoordinator {
    pub fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        usb_manager: UsbSerialManager,
    ) -> anyhow::Result<Self> {
        let mut db_ = db.lock().unwrap();

        event!(Level::DEBUG, "loading core coordinator");
        let mut coordinator = Persisted::<FrostCoordinator>::new(&mut db_, ())?;
        event!(Level::DEBUG, "loading device names");
        let device_names = Persisted::<DeviceNames>::new(&mut db_, ())?;

        // TODO: Make it possible to recover signing sessions.
        coordinator.staged_mutate(&mut *db_, |coordinator| {
            coordinator.cancel_all_signing_sessions();
            Ok(())
        })?;

        let usb_sender = usb_manager.usb_sender();
        let firmware_bin = usb_manager.upgrade_bin();

        let usb_manager = Mutex::new(Some(usb_manager));
        drop(db_);

        Ok(Self {
            usb_manager,
            thread_handle: Default::default(),
            key_event_stream: Default::default(),
            ui_protocol: Default::default(),
            firmware_upgrade_progress: Default::default(),
            recoverable_keys: Default::default(),
            device_list: Default::default(),
            device_list_stream: Default::default(),
            usb_sender,
            firmware_bin,
            db,
            coordinator: Arc::new(Mutex::new(coordinator)),
            device_names: Arc::new(Mutex::new(device_names)),
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
        let coordinator_loop = self.coordinator.clone();
        let ui_protocol = self.ui_protocol.clone();
        let db_loop = self.db.clone();
        let device_names = self.device_names.clone();
        let usb_sender = self.usb_sender.clone();
        let firmware_upgrade_progress = self.firmware_upgrade_progress.clone();
        let key_event_stream = self.key_event_stream.clone();
        let recoverable_keys = self.recoverable_keys.clone();
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
                                        firmware_upgrade_pogress.add(progress);
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
                let mut coordinator_outbox = VecDeque::default();
                let mut messages_from_devices = vec![];
                let mut db = db_loop.lock().unwrap();
                let mut device_list = device_list.lock().unwrap();

                // process new messages from devices
                {
                    for change in device_changes {
                        device_list.consume_manager_event(change.clone());
                        match change {
                            DeviceChange::Registered { id, .. } => {
                                if let Some(protocol) = &mut *ui_protocol_loop {
                                    protocol.connected(id);
                                }

                                if let Some(connected_device) = device_list.get_device(id) {
                                    // we only send some messages out if the device has up to date firmware
                                    if !connected_device.needs_firmware_upgrade().0 {
                                        coordinator_outbox.extend(
                                            coordinator.maybe_request_nonce_replenishment(
                                                id,
                                                &mut rand::thread_rng(),
                                            ),
                                        );
                                        coordinator_outbox
                                            .extend(coordinator.request_held_shares(id));
                                    }
                                }
                            }
                            DeviceChange::Disconnected { id } => {
                                if let Some(protocol) = &mut *ui_protocol_loop {
                                    protocol.disconnected(id);
                                }
                                let mut recoverable_keys = recoverable_keys.lock().unwrap();
                                let mut recoverable_list_changed = false;
                                for recoverable_shares in recoverable_keys.values_mut() {
                                    recoverable_shares.retain(|recoverable_share| {
                                        let remove = recoverable_share.held_by == id;
                                        recoverable_list_changed |= remove;
                                        !remove
                                    });
                                }

                                if recoverable_list_changed {
                                    if let Some(stream) = &*key_event_stream.lock().unwrap() {
                                        stream.add(key_state(&recoverable_keys, &coordinator));
                                    }
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
                            _ => { /* ignore rest */ }
                        }
                    }

                    if device_list.update_ready() {
                        if let Some(device_list_stream) = &*device_list_stream.lock().unwrap() {
                            device_list_stream.add(device_list.take_update());
                        }
                    }

                    if let Some(ui_protocol) = &mut *ui_protocol_loop {
                        for message in ui_protocol.poll() {
                            usb_sender.send(message);
                        }

                        Self::try_finish_protocol(
                            usb_sender.clone(),
                            coordinator.MUTATE_NO_PERSIST(),
                            &mut ui_protocol_loop,
                        );
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
                        AppMessageBody::AckUpgradeMode => {
                            if let Some(ui_protocol) = &mut *ui_protocol_loop {
                                ui_protocol.process_upgrade_mode_ack(app_message.from);
                            }
                        }
                    }
                }

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
                            match msg {
                                // there is no UI protocol for share recovery because it happens in the background.
                                CoordinatorToUserMessage::PromptRecoverShare(recover_share) => {
                                    let span = span!(
                                        Level::INFO,
                                        "recovering share",
                                        from = recover_share.held_by.to_string(),
                                        key_name = recover_share.held_share.key_name,
                                        access_structure_ref = format!(
                                            "{:?}",
                                            recover_share.held_share.access_structure_ref
                                        ),
                                    );
                                    let _enter = span.enter();
                                    let access_structure_ref =
                                        recover_share.held_share.access_structure_ref;
                                    let key_id = access_structure_ref.key_id;
                                    if coordinator.get_frost_key(key_id).is_some() {
                                        event!(Level::INFO, "share was for an existing key");
                                        // we don't need to the user to do anything here if they've already agreed to recover this key
                                        let result = coordinator.staged_mutate(
                                            &mut *db,
                                            |coordinator| {
                                                // TODO We're going to have to fetch a fresh encryption key from secure element here.
                                                // We can do this without bothering the user:
                                                // - generate a ChaCha key here
                                                // - generate a asymmetric key from phone secure element
                                                // - encrypt the ChaCha key to asymmetri key
                                                // - save the encrypted ChaCha key in our database
                                                // - Now only when we want to decrypt we need to ask user to put in pin
                                                coordinator
                                                    .recover_share_and_maybe_recover_access_structure(*recover_share.clone(), TEMP_KEY, &mut thread_rng())?;
                                                Ok(())
                                            },
                                        );

                                        if let Err(e) = result {
                                            event!(
                                                Level::ERROR,
                                                from = recover_share.held_by.to_string(),
                                                share_index = recover_share
                                                    .held_share
                                                    .share_image
                                                    .share_index
                                                    .to_string(),
                                                key_id = recover_share
                                                    .held_share
                                                    .access_structure_ref
                                                    .key_id
                                                    .to_string(),
                                                error = e.to_string(),
                                                "failed to recover share (or access structure)"
                                            );
                                        }
                                    } else {
                                        event!(
                                            Level::INFO,
                                            "recovery of this key has not been confirmed. Marking share as recoverable."
                                        );
                                        let mut recoverable_keys = recoverable_keys.lock().unwrap();
                                        let shares = recoverable_keys
                                            .entry(recover_share.held_share.access_structure_ref)
                                            .or_default();

                                        if !shares.contains(&recover_share) {
                                            shares.push(*recover_share);
                                        }
                                    }

                                    if let Some(stream) = &*key_event_stream.lock().unwrap() {
                                        let recoverable_keys = recoverable_keys.lock().unwrap();
                                        stream.add(key_state(&recoverable_keys, &coordinator));
                                    }
                                }
                                _ => {
                                    if let Some(ui_protocol) = &mut *ui_protocol_loop {
                                        ui_protocol.process_to_user_message(msg);
                                    } else {
                                        event!(
                                            Level::WARN,
                                            kind = msg.kind(),
                                            "ignoring protocol message we have no ui protoocl to handle"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        *self.thread_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn sub_key_events(&self, stream: StreamSink<api::KeyState>) {
        let mut key_event_stream = self.key_event_stream.lock().unwrap();
        let state = self.key_state();
        stream.add(state);
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
        purpose: KeyPurpose,
        sink: StreamSink<frostsnap_coordinator::keygen::KeyGenState>,
    ) -> anyhow::Result<()> {
        let currently_connected = self
            .device_list
            .lock()
            .unwrap()
            .devices()
            .into_iter()
            .map(|device| device.id)
            .collect();

        let do_keygen = DoKeyGen::new(devices, threshold, key_name, purpose);

        let ui_protocol = frostsnap_coordinator::keygen::KeyGen::new(
            SinkWrap(sink),
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            currently_connected,
            do_keygen,
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
        task: SignTask,
        sink: StreamSink<api::SigningState>,
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
        let mut ui_protocol = frostsnap_coordinator::signing::SigningDispatcher::new(
            devices,
            session_id,
            SinkWrap(sink),
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
        let mut proto = self.ui_protocol.lock().unwrap();
        let signing = proto
            .as_mut()
            .ok_or(anyhow!("No UI protocol running"))?
            .as_mut_any()
            .downcast_mut::<frostsnap_coordinator::signing::SigningDispatcher>()
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
        sink: StreamSink<api::SigningState>,
    ) -> anyhow::Result<()> {
        let coordinator = self.coordinator.lock().unwrap();

        let active_sign_session = coordinator
            .active_signing_sessions()
            .get(&session_id)
            .ok_or(anyhow!("this signing session no longer exists"))?;
        let mut dispatcher =
            frostsnap_coordinator::signing::SigningDispatcher::restore_signing_session(
                active_sign_session,
                SinkWrap(sink),
            );

        dispatcher.emit_state();
        self.start_protocol(dispatcher);

        Ok(())
    }

    // TODO: create method to return some state of the sign sign session
    pub fn active_signing_sessions(&self, key_id: KeyId) -> SyncReturn<Vec<SignSessionId>> {
        SyncReturn(
            self.coordinator
                .lock()
                .unwrap()
                .active_signing_sessions()
                .iter()
                .filter(|(_, sign_session)| sign_session.key_id == key_id)
                .map(|(ssid, _)| *ssid)
                .collect(),
        )
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
                .filter(|device| device.needs_firmware_upgrade().0)
                .map(|device| device.id)
                .collect();

            let ui_protocol =
                FirmwareUpgradeProtocol::new(devices, need_upgrade, firmware_bin, SinkWrap(sink));
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
        event!(Level::INFO, "Starting UI protocol {}", protocol.name());
        for device in self.device_list.lock().unwrap().devices() {
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
            stream.add(key_state(
                &self.recoverable_keys.lock().unwrap(),
                &coordinator,
            ));
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
        encryption_key: SymmetricKey,
    ) -> anyhow::Result<()> {
        let check_share_protocol = CheckShareProtocol::new(
            self.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            device_id,
            access_structure_ref,
            SinkWrap(stream),
            encryption_key,
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

    pub fn verify_address(
        &self,
        key_id: KeyId,
        address_index: u32,
        stream: StreamSink<api::VerifyAddressProtocolState>,
    ) -> anyhow::Result<()> {
        let coordinator = self.coordinator.lock().unwrap();

        let verify_address_messages = coordinator.verify_address(key_id, address_index)?;

        let ui_protocol =
            VerifyAddressProtocol::new(verify_address_messages.clone(), SinkWrap(stream));

        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn key_state(&self) -> api::KeyState {
        key_state(
            &self.recoverable_keys.lock().unwrap(),
            &self.coordinator.lock().unwrap(),
        )
    }

    pub fn start_recovery(&self, key_id: KeyId) -> Result<()> {
        let mut recoverable_keys = self.recoverable_keys.lock().unwrap();
        let recover_shares_by_as = recoverable_keys
            .range(AccessStructureRef::range_for_key(key_id))
            .map(|(k, v)| (*k, v.clone()))
            .collect::<Vec<_>>();

        let mut coordinator = self.coordinator.lock().unwrap();
        let mut db = self.db.lock().unwrap();
        coordinator.staged_mutate(&mut *db, |coordinator| {
            for (access_structure_ref, recover_shares) in recover_shares_by_as.clone() {
                for recover_share in recover_shares {
                    coordinator.recover_share_and_maybe_recover_access_structure(
                        recover_share,
                        TEMP_KEY,
                        &mut rand::thread_rng(),
                    )?;
                }
                recoverable_keys.remove(&access_structure_ref);
            }
            Ok(())
        })?;

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.add(key_state(&recoverable_keys, &coordinator));
        }

        Ok(())
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
            stream.add(self.key_state());
        }

        Ok(())
    }

    pub fn sub_device_events(&self, new_stream: StreamSink<api::DeviceListUpdate>) {
        let mut device_list_stream = self.device_list_stream.lock().unwrap();
        let mut device_list = self.device_list.lock().unwrap();
        new_stream.add(device_list.take_update());
        if let Some(old_stream) = device_list_stream.replace(new_stream) {
            old_stream.close();
        }
    }

    pub fn device_at_index(&self, index: usize) -> Option<api::ConnectedDevice> {
        self.device_list.lock().unwrap().device_at_index(index)
    }

    pub fn device_list_state(&self) -> api::DeviceListState {
        self.device_list.lock().unwrap().take_update().state
    }

    pub fn get_connected_device(&self, id: DeviceId) -> Option<api::ConnectedDevice> {
        self.device_list.lock().unwrap().get_device(id)
    }

    pub fn wipe_device_data(&self, id: DeviceId) {
        self.usb_sender.wipe_device_data(id);
    }

    pub fn cancel_sign_sesssion(&self, ssid: SignSessionId) -> anyhow::Result<()> {
        let mut db = self.db.lock().unwrap();
        event!(
            Level::INFO,
            ssid = ssid.to_string(),
            "canceling sign session"
        );
        self.coordinator
            .lock()
            .unwrap()
            .staged_mutate(&mut *db, |coordinator| {
                coordinator.cancel_sign_session(ssid);
                Ok(())
            })?;
        Ok(())
    }
}

fn key_state(
    recoverable_keys: &BTreeMap<AccessStructureRef, Vec<RecoverShare>>,
    coordinator: &FrostCoordinator,
) -> api::KeyState {
    let keys = coordinator
        .iter_keys()
        .cloned()
        .map(|coord_key| api::FrostKey(RustOpaque::new(coord_key)))
        .collect();

    let recoverable = recoverable_keys
        .values()
        .filter_map(|recover_shares| {
            let first = &recover_shares.first()?.held_share;
            Some(api::RecoverableKey {
                name: first.key_name.clone(),
                threshold: first.threshold,
                access_structure_ref: first.access_structure_ref,
                shares_obtained: recover_shares
                    .iter()
                    .map(|recover_share| recover_share.held_share.share_image.share_index)
                    .collect::<BTreeSet<_>>()
                    .len() as u16,
            })
        })
        .collect();
    api::KeyState { keys, recoverable }
}
