use crate::api::{self, KeyState};
use crate::persist_core::PersistCore;
use anyhow::{anyhow, Context, Result};
use flutter_rust_bridge::{RustOpaque, StreamSink};
use frostsnap_coordinator::backup::BackupProtocol;
use frostsnap_coordinator::firmware_upgrade::{
    FirmwareUpgradeConfirmState, FirmwareUpgradeProtocol,
};
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination, FirmwareDigest,
};
use frostsnap_coordinator::frostsnap_core::message::{
    CoordinatorSend, CoordinatorToStorageMessage,
};
use frostsnap_coordinator::keygen::KeyGenState;
use frostsnap_coordinator::signing::SigningState;
use frostsnap_coordinator::{
    frostsnap_core, AppMessageBody, FirmwareBin, UiProtocol, UsbSender, UsbSerialManager,
};
use frostsnap_coordinator::{Completion, DeviceChange};
use frostsnap_core::{DeviceId, FrostCoordinator, Gist, KeyId, SignTask};
use llsdb::{IndexHandle, LlsDb};
use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{event, Level};

pub struct FfiCoordinator {
    coordinator: Arc<Mutex<FrostCoordinator>>,
    usb_manager: Mutex<Option<UsbSerialManager>>,
    pending_for_outbox: Arc<Mutex<VecDeque<CoordinatorSend>>>,
    key_event_stream: Arc<Mutex<Option<StreamSink<KeyState>>>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    ui_protocol: Arc<Mutex<Option<Box<dyn UiProtocol>>>>,
    db: Arc<Mutex<LlsDb<File>>>,
    persist_core: IndexHandle<PersistCore>,
    device_names: IndexHandle<llsdb::index::BTreeMap<DeviceId, String>>,
    usb_sender: UsbSender,
    firmware_bin: FirmwareBin,
    firmware_upgrade_progress: Arc<Mutex<Option<StreamSink<f32>>>>,
}

impl FfiCoordinator {
    pub fn new(
        db: Arc<Mutex<LlsDb<File>>>,
        mut usb_manager: UsbSerialManager,
    ) -> anyhow::Result<Self> {
        let pending_for_outbox = Arc::new(Mutex::new(VecDeque::new()));

        let (persist_core, device_names_handle, coordinator) = db
            .lock()
            .unwrap()
            .execute(|tx| {
                let persist = PersistCore::new(tx)?;
                let handle = tx.store_index(persist);
                let device_names_list = tx.take_list("device_names")?;
                let device_names_index = llsdb::index::BTreeMap::new(device_names_list, &tx)?;
                let device_names_handle = tx.store_index(device_names_index);
                let core_api = tx.take_index(handle);
                let coordinator = core_api.core_coordinator()?;
                let device_names = tx.take_index(device_names_handle);
                *usb_manager.device_labels_mut() = device_names
                    .iter()
                    .collect::<Result<_>>()
                    .context("reading in device names from disk")?;

                Ok((handle, device_names_handle, coordinator))
            })
            .context("initializing db")?;

        let usb_sender = usb_manager.usb_sender();

        // HACK: if the global device list depends on db state then it shouldn't be global! The
        // reason it needs these names is for convenience. There are too many places that have
        // copies of the device names -- we need a central location.
        crate::api::init_device_names(usb_manager.device_labels().clone());
        let firmware_bin = usb_manager.upgrade_bin();

        let usb_manager = Mutex::new(Some(usb_manager));

        Ok(Self {
            coordinator: Arc::new(Mutex::new(coordinator)),
            usb_manager,
            pending_for_outbox,
            thread_handle: Default::default(),
            key_event_stream: Default::default(),
            ui_protocol: Default::default(),
            firmware_upgrade_progress: Default::default(),
            db,
            persist_core,
            device_names: device_names_handle,
            usb_sender,
            firmware_bin,
        })
    }

    pub fn persist_core_handle(&self) -> IndexHandle<PersistCore> {
        self.persist_core
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
        let key_event_stream_loop = self.key_event_stream.clone();
        let db_loop = self.db.clone();
        let core_persist = self.persist_core;
        let device_names = self.device_names;
        let usb_sender = self.usb_sender.clone();
        let firmware_upgrade_progress = self.firmware_upgrade_progress.clone();

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
                            DeviceChange::NewUnknownDevice { id, name } => {
                                // TODO: We should be asking the user to accept the new device before writing anything to disk.
                                let res = db_loop
                                    .lock()
                                    .unwrap()
                                    .execute(|tx| tx.take_index(device_names).insert(*id, name));
                                if let Err(e) = res {
                                    event!(
                                        Level::ERROR,
                                        error = e.to_string(),
                                        "unable to save device name"
                                    );
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
                        if !to_storage.is_empty() {
                            let res = db_loop.lock().unwrap().execute(|tx| {
                                let mut persist = tx.take_index(core_persist);
                                for message in to_storage {
                                    persist.consume_ui_message(message)?;
                                }
                                Ok(())
                            });

                            if let Err(e) = res {
                                event!(
                                    Level::ERROR,
                                    "persitence failed to consume ui message: {e}"
                                );
                            }
                        }

                        if let Some(completion) = ui_protocol.is_complete() {
                            event!(
                                Level::INFO,
                                "UI Protocol {} completed with {:?}",
                                ui_protocol.name(),
                                completion
                            );
                            *ui_protocol_loop = None;
                            if let Completion::Abort = completion {
                                event!(Level::DEBUG, "canceling protocol due to abort");
                                coordinator.cancel();
                                usb_sender.send_cancel_all();
                            }
                        }
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
                            match coordinator.recv_device_message(app_message.from, core_message) {
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
                            };
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
                        CoordinatorSend::ToStorage(to_storage) => {
                            let update_kind = to_storage.gist();
                            let mut db = db_loop.lock().unwrap();
                            let res = db.execute(|tx| {
                                let mut persist = tx.take_index(core_persist);
                                persist.consume_core_message(to_storage.clone())?;
                                if let CoordinatorToStorageMessage::NewKey(_) = &to_storage {
                                    // Note we do this here rather than in the ToUserMessage
                                    // because the key list is persisted and so its better to
                                    // nofify the app after the on disk state is written.
                                    if let Some(stream) = &*key_event_stream_loop.lock().unwrap() {
                                        stream.add(KeyState {
                                            keys: frost_keys(&coordinator_loop.lock().unwrap()),
                                        });
                                    }
                                }
                                Ok(())
                            });

                            match res {
                                Ok(_) => {
                                    event!(
                                        Level::INFO,
                                        kind = update_kind,
                                        "Updated core persistence"
                                    )
                                }
                                Err(e) => event!(
                                    Level::ERROR,
                                    error = e.to_string(),
                                    kind = update_kind,
                                    "Failed to repsond to storage update"
                                ),
                            }
                        }
                    }
                }
            }
        });

        *self.thread_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn sub_key_events(&self, stream: StreamSink<KeyState>) {
        let mut key_event_stream = self.key_event_stream.lock().unwrap();
        stream.add(KeyState {
            keys: self.frost_keys(),
        });
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

    pub fn cancel_all(&self) {
        self.coordinator.lock().unwrap().cancel();
        self.usb_sender.send_cancel_all()
    }

    pub fn generate_new_key(
        &self,
        devices: BTreeSet<DeviceId>,
        threshold: usize,
        sink: StreamSink<frostsnap_coordinator::keygen::KeyGenState>,
    ) -> anyhow::Result<()> {
        let ui_protocol =
            frostsnap_coordinator::keygen::KeyGen::new(SinkWrap(sink), devices.clone(), threshold);

        let keygen_messages = {
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.do_keygen(&devices, threshold as u16)?
        };

        self.pending_for_outbox
            .lock()
            .unwrap()
            .extend(keygen_messages);

        ui_protocol.emit_state();
        self.start_protocol(ui_protocol);

        Ok(())
    }

    pub fn frost_keys(&self) -> Vec<crate::api::FrostKey> {
        frost_keys(&self.coordinator.lock().unwrap())
    }

    pub fn nonces_left(&self, id: DeviceId) -> Option<usize> {
        self.coordinator
            .lock()
            .unwrap()
            .device_nonces()
            .get(&id)
            .map(|nonces| nonces.nonces.len())
    }

    pub fn start_signing(
        &self,
        key_id: KeyId,
        devices: BTreeSet<DeviceId>,
        task: SignTask,
        sink: StreamSink<api::SigningState>,
    ) -> anyhow::Result<()> {
        let mut coordinator = self.coordinator.lock().unwrap();
        let mut messages = coordinator.start_sign(key_id, task, devices.clone())?;
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
        #[allow(unused)] /* we only have one key for now */ key_id: KeyId,
        sink: StreamSink<api::SigningState>,
    ) -> anyhow::Result<()> {
        let signing_session_state = self
            .db
            .lock()
            .unwrap()
            .execute(|tx| tx.take_index(self.persist_core).persisted_signing())?;

        let signing_session_state =
            signing_session_state.ok_or(anyhow!("no signing session to restore"))?;
        let mut coordinator = self.coordinator.lock().unwrap();
        coordinator.restore_sign_session(signing_session_state.clone());

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
    ) -> Result<Option<api::SignTaskDescription>> {
        self.db.lock().unwrap().execute(|tx| {
            Ok(tx
                .take_index(self.persist_core)
                .persisted_sign_session_task(key_id)?
                .map(|task| match task {
                    SignTask::Plain { message, .. } => api::SignTaskDescription::Plain { message },
                    SignTask::Nostr { .. } => todo!("nostr restoring not yet implemented"),
                    SignTask::BitcoinTransaction(task) => api::SignTaskDescription::Transaction {
                        unsigned_tx: api::UnsignedTx {
                            template_tx: RustOpaque::new(task),
                        },
                    },
                }))
        })
    }

    pub fn request_display_backup(
        &self,
        device_id: DeviceId,
        key_id: KeyId,
        stream: StreamSink<()>,
    ) -> anyhow::Result<()> {
        // XXX: We should be storing the id to make sure the device that sends the backup ack is
        // from the one we expected. In practice it doesn't matter that much and flutter rust bridge
        // was giving me extreme grief. Can try again with frb v2.
        let backup_protocol = BackupProtocol::new(device_id, SinkWrap(stream));

        let messages = self
            .coordinator
            .lock()
            .unwrap()
            .request_device_display_backup(device_id, key_id)?;
        self.pending_for_outbox.lock().unwrap().extend(messages);

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
        if let Some(mut prev) = self.ui_protocol.lock().unwrap().replace(Box::new(protocol)) {
            prev.cancel();
        }
    }

    pub fn cancel_protocol(&self) {
        if let Some(proto) = &mut *self.ui_protocol.lock().unwrap() {
            proto.cancel();
        }
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
}

fn frost_keys(coordinator: &FrostCoordinator) -> Vec<crate::api::FrostKey> {
    coordinator
        .iter_keys()
        .map(|coord_frost_key| crate::api::FrostKey(RustOpaque::new(coord_frost_key.clone())))
        .collect()
}

// we need to wrap it so we can impl it on foreign FRB type. You can't do a single generic impl. Try
// it if you don't believe me.
struct SinkWrap<T>(StreamSink<T>);

macro_rules! bridge_sink {
    ($type:ty) => {
        impl frostsnap_coordinator::Sink<$type> for SinkWrap<$type> {
            fn send(&self, state: $type) {
                self.0.add(state);
            }

            fn close(&self) {
                self.0.close();
            }
        }
    };
}

bridge_sink!(KeyGenState);
bridge_sink!(FirmwareUpgradeConfirmState);
bridge_sink!(SigningState);
bridge_sink!(());
