use crate::api::{self};
use crate::device_list::DeviceList;
use crate::sink_wrap::SinkWrap;
use crate::TEMP_KEY;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::{RustOpaque, StreamSink};
use frostsnap_coordinator::display_backup::DisplayBackupProtocol;
use frostsnap_coordinator::enter_physical_backup::{EnterPhysicalBackup, EnterPhysicalBackupState};
use frostsnap_coordinator::firmware_upgrade::{
    FirmwareUpgradeConfirmState, FirmwareUpgradeProtocol,
};
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination, Sha256Digest,
};
use frostsnap_coordinator::frostsnap_core::coordinator::restoration::PhysicalBackupPhase;
use frostsnap_coordinator::frostsnap_core::coordinator::{
    CoordAccessStructure, CoordFrostKey, CoordinatorSend,
};
use frostsnap_coordinator::frostsnap_core::device::KeyPurpose;
use frostsnap_coordinator::frostsnap_core::{
    self, message::DoKeyGen, Kind as _, RestorationId, SignSessionId,
};
use frostsnap_coordinator::frostsnap_core::{KeygenId, SymmetricKey};
use frostsnap_coordinator::frostsnap_persist::DeviceNames;
use frostsnap_coordinator::persist::Persisted;
use frostsnap_coordinator::signing::SigningState;
use frostsnap_coordinator::verify_address::VerifyAddressProtocol;
use frostsnap_coordinator::wait_for_recovery_share::{
    WaitForRecoveryShare, WaitForRecoveryShareState,
};
use frostsnap_coordinator::{
    AppMessageBody, DeviceMode, FirmwareBin, Sink, UiProtocol, UsbSender, UsbSerialManager,
};
use frostsnap_coordinator::{Completion, DeviceChange};
use frostsnap_core::{
    coordinator::{
        restoration::{RecoverShare, RestorationState},
        FrostCoordinator,
    },
    AccessStructureRef, DeviceId, KeyId, WireSignTask,
};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{event, Level};
const N_NONCE_STREAMS: usize = 4;

pub struct FfiCoordinator {
    usb_manager: Mutex<Option<UsbSerialManager>>,
    key_event_stream: Arc<Mutex<Option<StreamSink<api::KeyState>>>>,
    signing_session_signals: Arc<Mutex<HashMap<KeyId, Signal>>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    ui_protocol: Arc<Mutex<Option<Box<dyn UiProtocol>>>>,
    usb_sender: UsbSender,
    firmware_bin: Option<FirmwareBin>,
    firmware_upgrade_progress: Arc<Mutex<Option<StreamSink<f32>>>>,

    device_list: Arc<Mutex<DeviceList>>,
    device_list_stream: Arc<Mutex<Option<StreamSink<api::DeviceListUpdate>>>>,

    // persisted things
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
            ui_protocol: Default::default(),
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
        let ui_protocol = self.ui_protocol.clone();
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
                let mut coordinator_outbox = VecDeque::default();
                let mut messages_from_devices = vec![];
                let mut db = db_loop.lock().unwrap();
                let mut ui_protocol_loop = ui_protocol.lock().unwrap();

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
                                if let Some(protocol) = &mut *ui_protocol_loop {
                                    protocol.connected(
                                        id,
                                        device_list
                                            .get_device(id)
                                            .expect("it was just registered")
                                            .device_mode(),
                                    );
                                }

                                if let Some(connected_device) = device_list.get_device(id) {
                                    // we only send some messages out if the device has up to date firmware
                                    if !connected_device.needs_firmware_upgrade().0 {
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
                                if let Some(protocol) = &mut *ui_protocol_loop {
                                    protocol.disconnected(id);
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
                                if let Some(protocol) = &mut *ui_protocol_loop {
                                    protocol.connected(id, DeviceMode::Blank);
                                }
                            }
                            _ => { /* ignore rest */ }
                        }
                    }

                    if device_list.update_ready() {
                        if let Some(device_list_stream) = &*device_list_stream.lock().unwrap() {
                            device_list_stream.add(device_list.take_update());
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
                            if let Some(ui_protocol) = &mut *ui_protocol_loop {
                                ui_protocol.process_comms_message(app_message.from, comms_misc);
                            }
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

                // poll the ui protocol first before locking anything else because of the potential
                // for dead locks with callbacks activated on stream items trying to lock things.
                if let Some(ui_protocol) = &mut *ui_protocol_loop {
                    for message in ui_protocol.poll() {
                        usb_sender.send(message);
                    }

                    Self::try_finish_protocol(usb_sender.clone(), &mut ui_protocol_loop);
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

        let do_keygen = DoKeyGen::new(
            devices,
            threshold,
            key_name,
            purpose,
            &mut rand::thread_rng(),
        );

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
            protocol.connected(device.id, device.device_mode());
        }
        event!(Level::INFO, "TMP {}", protocol.name());

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

        event!(Level::INFO, "Started UI protocol");
    }

    pub fn cancel_protocol(&self) {
        let mut proto_opt = self.ui_protocol.lock().unwrap();
        if let Some(proto) = &mut *proto_opt {
            proto.cancel();
            assert!(
                Self::try_finish_protocol(self.usb_sender.clone(), &mut proto_opt),
                "protocol must be finished after cancel"
            );
        }
    }

    fn try_finish_protocol(
        usb_sender: UsbSender,
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

    pub fn final_keygen_ack(&self, keygen_id: KeygenId) -> Result<AccessStructureRef> {
        let mut coordinator = self.coordinator.lock().unwrap();
        let mut db = self.db.lock().unwrap();

        let mut proto = self.ui_protocol.lock().unwrap();
        let keygen = proto
            .as_mut()
            .ok_or(anyhow!("No UI protocol running"))?
            .as_mut_any()
            .downcast_mut::<frostsnap_coordinator::keygen::KeyGen>()
            .ok_or(anyhow!("somehow UI was not in KeyGen state"))?;

        let accs_ref = coordinator.staged_mutate(&mut db, |coordinator| {
            Ok(coordinator.final_keygen_ack(keygen_id, TEMP_KEY, &mut rand::thread_rng())?)
        })?;

        keygen.final_keygen_ack(accs_ref);

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.add(key_state(&coordinator));
        }

        assert!(
            Self::try_finish_protocol(self.usb_sender.clone(), &mut proto),
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
            stream.add(self.key_state());
        }

        Ok(restoration_id)
    }

    pub fn start_restoring_wallet_from_device_share(
        &self,
        recover_share: RecoverShare,
    ) -> Result<RestorationId> {
        let restoration_id = {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                let restoration_id = RestorationId::new(&mut rand::thread_rng());
                coordinator.start_restoring_key_from_recover_share(recover_share, restoration_id);
                Ok(restoration_id)
            })?
        };

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.add(self.key_state());
        }

        Ok(restoration_id)
    }

    pub fn continue_restoring_wallet_from_device_share(
        &self,
        restoration_id: RestorationId,
        recover_share: RecoverShare,
    ) -> Result<()> {
        {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                coordinator.add_recovery_share_to_restoration(restoration_id, recover_share)?;
                Ok(())
            })?;
        }

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.add(self.key_state());
        }
        Ok(())
    }

    pub fn recover_share(
        &self,
        recover_share: RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        {
            {
                let mut db = self.db.lock().unwrap();
                let mut coordinator = self.coordinator.lock().unwrap();
                coordinator.staged_mutate(&mut *db, |coordinator| {
                    coordinator.recover_share(recover_share, encryption_key)?;
                    Ok(())
                })?;
            }

            if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
                stream.add(self.key_state());
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
        let assid = {
            let mut db = self.db.lock().unwrap();
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                Ok(coordinator.finish_restoring(
                    restoration_id,
                    encryption_key,
                    &mut rand::thread_rng(),
                )?)
            })?
        };

        if let Some(stream) = &*self.key_event_stream.lock().unwrap() {
            stream.add(self.key_state());
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
            stream.add(key_state(&coordinator));
        }

        Ok(())
    }

    pub fn sub_signing_session_signals(&self, key_id: KeyId, new_stream: impl Sink<()>) {
        let mut signal_streams = self.signing_session_signals.lock().unwrap();
        if let Some(old_steam) = signal_streams.insert(key_id, Box::new(new_stream)) {
            old_steam.close();
        }
    }

    pub fn emit_signing_signal(&self, key_id: KeyId) {
        let signal_streams = self.signing_session_signals.lock().unwrap();
        if let Some(stream) = signal_streams.get(&key_id) {
            stream.send(())
        }
    }

    pub fn tell_device_to_enter_physical_backup_and_save(
        &self,
        restoration_id: RestorationId,
        device_id: DeviceId,
        sink: impl Sink<EnterPhysicalBackupState>,
    ) -> Result<()> {
        let device_list = self.device_list.clone();
        let coordinator = self.coordinator.clone();
        let key_event_stream = self.key_event_stream.clone();
        let sink = sink.inspect(move |state| {
            if state.saved == Some(true) {
                // When the physical backup has been saved on the device this means the device has
                // entered recovery mode.
                device_list
                    .lock()
                    .unwrap()
                    .set_recovery_mode(device_id, true);

                let coordinator = coordinator.lock().unwrap();
                let state = key_state(&coordinator);
                if let Some(key_event_stream) = &*key_event_stream.lock().unwrap() {
                    key_event_stream.add(state);
                }
            }
        });
        let proto = EnterPhysicalBackup::new(sink, restoration_id, device_id, true);
        self.start_protocol(proto);
        Ok(())
    }

    pub fn tell_device_to_enter_physical_backup(
        &self,
        device_id: DeviceId,
        sink: impl Sink<EnterPhysicalBackupState>,
    ) -> Result<()> {
        let restoration_id = RestorationId::new(&mut rand::thread_rng());
        let proto = EnterPhysicalBackup::new(sink, restoration_id, device_id, false);
        self.start_protocol(proto);
        Ok(())
    }

    /// This is for telling a device that a physical backup it just entered is
    pub fn tell_device_to_consolidate_physical_backup(
        &self,
        access_structure_ref: AccessStructureRef,
        phase: PhysicalBackupPhase,
        encryption_key: SymmetricKey,
    ) -> Result<()> {
        let mut coordinator = self.coordinator.lock().unwrap();
        let msgs = coordinator
            .MUTATE_NO_PERSIST()
            .tell_device_to_consolidate_physical_backup(
                phase,
                access_structure_ref,
                encryption_key,
            )?
            .into_iter()
            .map(|msg| CoordinatorSendMessage::try_from(msg).unwrap());

        for msg in msgs {
            self.usb_sender.send(msg);
        }
        // NOTE: We're not waiting for the consolidation ack here. Persisted mutations will happen
        // occur once we get the ack. The caller doesn't really care as long as the consolidation
        // succeeds (hopefully) eventually.
        Ok(())
    }

    pub fn exit_recovery_mode(&self, device_id: DeviceId, encryption_key: SymmetricKey) {
        let coord = self.coordinator.lock().unwrap();
        let mut device_list = self.device_list.lock().unwrap();

        if let Some(device) = device_list.get_device(device_id) {
            let msgs = coord
                .consolidate_pending_physical_backups(device_id, encryption_key)
                .into_iter()
                .map(|msg| CoordinatorSendMessage::try_from(msg).unwrap());

            for msg in msgs {
                self.usb_sender.send(msg);
            }

            // NOTE: We don't wait for the device to confirm that they have exited recovery mode
            // because we expect everything to go well after we have sent the messages. Internally
            // thought the core coordinator will mark that device as having left recovery mode only
            // once it's confirmed.
            device_list.set_recovery_mode(device_id, false);

            // We finally connect the device properly!
            if let Some(ui_protocol) = &mut *self.ui_protocol.lock().unwrap() {
                ui_protocol.connected(device_id, device.device_mode());
            }
        }
    }
}

fn key_state(coordinator: &FrostCoordinator) -> api::KeyState {
    let keys = coordinator
        .iter_keys()
        .cloned()
        .map(|coord_key| api::FrostKey(RustOpaque::new(coord_key)))
        .collect();

    let restoring = coordinator
        .restoring()
        .map(|restoring| api::RestoringKey {
            restoration_id: restoring.restoration_id,
            name: restoring.key_name,
            threshold: restoring.access_structure.threshold,
            shares_obtained: restoring
                .access_structure
                .share_images
                .keys()
                .copied()
                .collect(),
            bitcoin_network: restoring.key_purpose.bitcoin_network().map(From::from),
        })
        .collect();

    api::KeyState { keys, restoring }
}
