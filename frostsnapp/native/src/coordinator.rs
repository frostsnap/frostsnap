use crate::api::{self, FfiSerial, KeyState, PortEvent};
use crate::persist_core::PersistCore;
use crate::SigningSession;
use anyhow::{anyhow, Context, Result};
use flutter_rust_bridge::{RustOpaque, StreamSink};
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination,
};
use frostsnap_coordinator::frostsnap_core::message::{
    CoordinatorSend, CoordinatorToStorageMessage, CoordinatorToUserKeyGenMessage,
    CoordinatorToUserMessage, SignTask,
};
use frostsnap_coordinator::frostsnap_core::KeyId;
use frostsnap_coordinator::{
    frostsnap_core, PortChanges, PortDesc, PortOpenError, Serial, SerialPort, UsbSerialManager,
};
use frostsnap_coordinator::{serialport, DeviceChange};
use frostsnap_coordinator::{SigningDispatcher, UsbSender};
use frostsnap_core::{DeviceId, FrostCoordinator, Gist};
use llsdb::{IndexHandle, LlsDb};
use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{event, Level};

pub struct FfiCoordinator {
    coordinator: Arc<Mutex<FrostCoordinator>>,
    usb_manager: Mutex<Option<UsbSerialManager>>,
    pending_for_outbox: Arc<Mutex<VecDeque<CoordinatorSend>>>,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    keygen_stream: Arc<Mutex<Option<StreamSink<CoordinatorToUserKeyGenMessage>>>>,
    key_event_stream: Arc<Mutex<Option<StreamSink<KeyState>>>>,
    signing_session: Arc<Mutex<Option<SigningSession>>>,
    db: Arc<Mutex<LlsDb<File>>>,
    persist_core: IndexHandle<PersistCore>,
    device_names: IndexHandle<llsdb::index::BTreeMap<DeviceId, String>>,
    usb_sender: UsbSender,
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
                let (handle, api) = tx.store_and_take_index(persist);
                let coordinator = api.core_coordinator()?;
                let device_names_list = tx.take_list("device_names")?;
                let device_names_index = llsdb::index::BTreeMap::new(device_names_list, &tx)?;
                let device_names_handle = tx.store_index(device_names_index);
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

        let usb_manager = Mutex::new(Some(usb_manager));

        Ok(Self {
            coordinator: Arc::new(Mutex::new(coordinator)),
            usb_manager,
            pending_for_outbox,
            thread_handle: Default::default(),
            keygen_stream: Default::default(),
            signing_session: Default::default(),
            key_event_stream: Default::default(),
            db,
            persist_core,
            device_names: device_names_handle,
            usb_sender,
        })
    }

    pub fn persist_core_handle(&self) -> IndexHandle<PersistCore> {
        self.persist_core.clone()
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
        let keygen_stream_loop = self.keygen_stream.clone();
        let signing_stream_loop = self.signing_session.clone();
        let key_event_stream_loop = self.key_event_stream.clone();
        let db_loop = self.db.clone();
        let core_persist = self.persist_core;
        let device_names = self.device_names;
        let usb_sender = self.usb_sender.clone();

        let handle = std::thread::spawn(move || {
            loop {
                // to give time for the other threads to get a lock
                std::thread::sleep(Duration::from_millis(100));

                let new_messages_from_devices = {
                    // NOTE: Never hold locks on anything over poll_ports because poll ports makes
                    // blocking calls up to flutter. If flutter is blocked on something else we'll
                    // be deadlocked.
                    let PortChanges {
                        device_changes,
                        new_messages,
                    } = usb_manager.poll_ports();

                    let mut signing_session = signing_stream_loop.lock().unwrap();

                    if let Some(signing_session) = &mut *signing_session {
                        if let Some(message) = signing_session.resend_sign_request() {
                            usb_sender.send(message);
                        }
                    }

                    if !device_changes.is_empty() {
                        for change in &device_changes {
                            match change {
                                DeviceChange::Registered { id, .. } => {
                                    if let Some(signing_session) = &mut *signing_session {
                                        signing_session.connected(*id);
                                    }
                                }
                                DeviceChange::Disconnected { id } => {
                                    if let Some(signing_session) = &mut *signing_session {
                                        signing_session.disconnected(*id);
                                    }
                                }
                                DeviceChange::NewUnknownDevice { id, name } => {
                                    // TODO: We should be asking the user to accept the new device before writing anything to disk.
                                    let res = db_loop.lock().unwrap().execute(|tx| {
                                        tx.take_index(device_names).insert(*id, name)
                                    });
                                    if let Err(e) = res {
                                        event!(
                                            Level::ERROR,
                                            error = e.to_string(),
                                            "unable to save device name"
                                        );
                                    }
                                }
                                _ => { /* ignore rest */ }
                            }
                        }

                        crate::api::emit_device_events(
                            device_changes
                                .into_iter()
                                .map(crate::api::DeviceChange::from)
                                .collect(),
                        );
                    }

                    new_messages
                };

                let mut coordinator = coordinator_loop.lock().unwrap();
                let mut coordinator_outbox = pending_loop.lock().unwrap();
                for (from, message) in new_messages_from_devices {
                    match coordinator.recv_device_message(from, message.clone()) {
                        Ok(messages) => {
                            coordinator_outbox.extend(messages);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                from = from.to_string(),
                                "Failed to process message: {}",
                                e
                            );
                        }
                    };
                }

                drop(coordinator);

                while let Some(message) = coordinator_outbox.pop_front() {
                    match message {
                        CoordinatorSend::ToDevice(msg) => {
                            let send_message = CoordinatorSendMessage {
                                target_destinations: Destination::from(msg.default_destinations()),
                                message_body: CoordinatorSendBody::Core(msg),
                            };

                            usb_sender.send(send_message);
                        }
                        CoordinatorSend::ToUser(msg) => match msg {
                            CoordinatorToUserMessage::KeyGen(event) => {
                                let mut stream_opt = keygen_stream_loop.lock().unwrap();
                                if let Some(stream) = stream_opt.as_ref() {
                                    let is_finished = matches!(
                                        event,
                                        CoordinatorToUserKeyGenMessage::FinishedKey { .. }
                                    );

                                    if !stream.add(event) {
                                        event!(Level::ERROR, "failed to emit keygen event");
                                    }

                                    if is_finished {
                                        stream.close();
                                        *stream_opt = None;
                                    }
                                }
                            }
                            CoordinatorToUserMessage::Signing(signing_message) => {
                                let mut signing_session = signing_stream_loop.lock().unwrap();
                                if let Some(signing_session) = &mut *signing_session {
                                    signing_session.process_to_user_message(signing_message);

                                    if signing_session.is_complete() {
                                        let _ = db_loop.lock().unwrap().execute(|tx| {
                                            tx.take_index(core_persist).clear_signing_session()
                                        });
                                        event!(Level::INFO, "received signatures from all devices");
                                    }
                                }
                            }
                        },
                        CoordinatorSend::ToStorage(to_storage) => {
                            let update_kind = to_storage.gist();
                            let mut db = db_loop.lock().unwrap();
                            let res = db.execute(|tx| {
                                let mut persist = tx.take_index(core_persist);
                                match to_storage {
                                    CoordinatorToStorageMessage::NewKey(new_key) => {
                                        // we only have one key so we just overwrite it
                                        persist.set_key_state(new_key)?;
                                        // signing sessions are not longer relevant.
                                        persist.clear_signing_session()?;
                                        // keygen is finished so we need to tell the global key list
                                        // that there's a new key.
                                        //
                                        // Note we do this here rather than in the ToUserMessage
                                        // because the key list is persisted and so its better to
                                        // nofify the app after the on disk state is written.
                                        if let Some(stream) =
                                            &*key_event_stream_loop.lock().unwrap()
                                        {
                                            stream.add(KeyState {
                                                keys: frost_keys(&coordinator_loop.lock().unwrap()),
                                            });
                                        }
                                    }
                                    CoordinatorToStorageMessage::StoreSigningState(sign_state) => {
                                        persist.store_sign_session(sign_state)?
                                    }
                                    CoordinatorToStorageMessage::UpdateFrostKey(state) => {
                                        persist.set_key_state(state)?
                                    }
                                }
                                Ok(())
                            });

                            match res {
                                Ok(_) => {
                                    event!(Level::INFO, kind = update_kind, "Updated persistence")
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
        if let Some(mut signing_session) = self.signing_session.lock().unwrap().take() {
            signing_session.cancel();
            let _ = self
                .db
                .lock()
                .unwrap()
                .execute(|tx| tx.take_index(self.persist_core).clear_signing_session());
        }
        self.coordinator.lock().unwrap().cancel();
        self.usb_sender.send_cancel_all()
    }

    pub fn generate_new_key(
        &self,
        devices: BTreeSet<DeviceId>,
        threshold: usize,
        stream: StreamSink<CoordinatorToUserKeyGenMessage>,
    ) {
        if let Some(existing) = self.keygen_stream.lock().unwrap().replace(stream) {
            existing.close();
        }
        let keygen_message = {
            let mut coordinator = self.coordinator.lock().unwrap();
            *coordinator = FrostCoordinator::default();
            coordinator.do_keygen(&devices, threshold).unwrap()
        };
        let keygen_message = CoordinatorSend::ToDevice(keygen_message);
        self.pending_for_outbox
            .lock()
            .unwrap()
            .push_back(keygen_message);
    }

    pub fn frost_keys(&self) -> Vec<crate::api::FrostKey> {
        frost_keys(&self.coordinator.lock().unwrap())
    }

    pub fn nonces_left(&self, id: DeviceId) -> Option<usize> {
        self.coordinator
            .lock()
            .unwrap()
            .frost_key_state()?
            .nonces_left(id)
    }

    pub fn start_signing(
        &self,
        _key_id: KeyId,
        devices: BTreeSet<DeviceId>,
        task: SignTask,
        stream: StreamSink<api::SigningState>,
    ) -> anyhow::Result<()> {
        // we need to lock this first to avoid race conditions where somehow get_signing_state is called before this completes.
        let mut signing_session = self.signing_session.lock().unwrap();
        let mut coordinator = self.coordinator.lock().unwrap();
        let mut messages = coordinator.start_sign(task, devices)?;
        let dispatcher = SigningDispatcher::from_filter_out_start_sign(&mut messages);
        let mut new_session = SigningSession::new(stream, dispatcher);

        for device in api::device_list_state().0.devices {
            new_session.connected(device.id);
        }

        self.pending_for_outbox.lock().unwrap().extend(messages);
        signing_session.replace(new_session);

        Ok(())
    }

    pub fn get_signing_state(&self) -> Option<api::SigningState> {
        let signing_session = self.signing_session.lock().unwrap();
        let state = signing_session.as_ref()?.signing_state();
        Some(state)
    }

    pub fn try_restore_signing_session(
        &self,
        #[allow(unused)] /* we only have one key for now */ key_id: KeyId,
        stream: StreamSink<api::SigningState>,
    ) -> anyhow::Result<()> {
        let signing_session = self
            .db
            .lock()
            .unwrap()
            .execute(|tx| tx.take_index(self.persist_core).persisted_signing())?;

        let signing_session_state =
            signing_session.ok_or(anyhow!("no signing session to restore"))?;
        let mut coordinator = self.coordinator.lock().unwrap();
        coordinator.restore_sign_session(signing_session_state.clone());

        let mut dispatcher =
            SigningDispatcher::new_from_request(signing_session_state.request.clone());

        for already_provided in signing_session_state.received_from() {
            dispatcher.set_signature_received(already_provided);
        }
        let mut session = SigningSession::new(stream, dispatcher);

        for device in api::device_list_state().0.devices {
            session.connected(device.id);
        }

        self.signing_session.lock().unwrap().replace(session);

        Ok(())
    }

    pub fn can_restore_signing_session(
        &self,
        #[allow(unused)] /* we only have one key for now */ key_id: KeyId,
    ) -> bool {
        self.db
            .lock()
            .unwrap()
            .execute(|tx| Ok(tx.take_index(self.persist_core).is_sign_session_persisted()))
            .unwrap()
    }
}

fn frost_keys(coordinator: &FrostCoordinator) -> Vec<crate::api::FrostKey> {
    coordinator
        .frost_key_state()
        .into_iter()
        .map(|key_state| crate::api::FrostKey(RustOpaque::new(key_state.clone())))
        .collect()
}

// Newtypes needed here because type aliases lead to weird types in the bindings
#[derive(Debug)]
pub struct PortOpenSender(pub SyncSender<Result<(), PortOpenError>>);
#[derive(Debug)]
pub struct PortWriteSender(pub SyncSender<Result<(), String>>);
#[derive(Debug)]
pub struct PortReadSender(pub SyncSender<Result<Vec<u8>, String>>);
#[derive(Debug)]
pub struct PortBytesToReadSender(pub SyncSender<u32>);

impl Serial for FfiSerial {
    fn available_ports(&self) -> Vec<PortDesc> {
        self.available_ports.lock().unwrap().clone()
    }

    fn open_device_port(&self, id: &str, baud_rate: u32) -> Result<SerialPort, PortOpenError> {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);

        crate::api::emit_event(PortEvent::Open {
            request: crate::api::PortOpen {
                id: id.into(),
                baud_rate,
                ready: RustOpaque::new(PortOpenSender(tx)),
            },
        })
        .map_err(|e| PortOpenError::Other(e.into()))?;

        rx.recv().map_err(|e| PortOpenError::Other(Box::new(e)))??;

        let port = FfiSerialPort {
            id: id.to_string(),
            baud_rate,
        };

        Ok(Box::new(port))
    }
}

pub struct FfiSerialPort {
    id: String,
    baud_rate: u32,
}

impl io::Read for FfiSerialPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Vec<u8>, String>>(0);
            crate::api::emit_event(PortEvent::Read {
                request: crate::api::PortRead {
                    id: self.id.clone(),
                    len: buf.len(),
                    ready: RustOpaque::new(PortReadSender(tx)),
                },
            })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e.to_string()))?;

            let result = rx.recv().unwrap();
            match result {
                Ok(bytes) => {
                    if !bytes.is_empty() {
                        buf[0..bytes.len()].copy_from_slice(&bytes);
                        return Ok(bytes.len());
                    } else {
                        // we got 0 bytes so wait a little while for more data
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
                Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
            }
        }
    }
}

impl std::io::Write for FfiSerialPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);

        crate::api::emit_event(PortEvent::Write {
            request: crate::api::PortWrite {
                id: self.id.clone(),
                bytes: buf.to_vec(),
                ready: RustOpaque::new(PortWriteSender(tx)),
            },
        })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e.to_string()))?;

        match rx.recv().unwrap() {
            Ok(()) => Ok(buf.len()),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        // assume FFI host will flush after each write
        Ok(())
    }
}

mod _impl {
    use super::serialport::*;
    use super::{PortBytesToReadSender, PortEvent, RustOpaque};

    #[allow(unused)]
    impl SerialPort for super::FfiSerialPort {
        fn name(&self) -> Option<String> {
            Some(self.id.clone())
        }
        fn baud_rate(&self) -> Result<u32> {
            Ok(self.baud_rate)
        }
        fn bytes_to_read(&self) -> Result<u32> {
            let (tx, rx) = std::sync::mpsc::sync_channel(0);

            crate::api::emit_event(PortEvent::BytesToRead {
                request: crate::api::PortBytesToRead {
                    id: self.id.clone(),
                    ready: RustOpaque::new(PortBytesToReadSender(tx)),
                },
            })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e.to_string()))?;

            Ok(rx.recv().unwrap())
        }

        fn data_bits(&self) -> Result<DataBits> {
            unimplemented!()
        }

        fn flow_control(&self) -> Result<FlowControl> {
            unimplemented!()
        }

        fn parity(&self) -> Result<Parity> {
            unimplemented!()
        }

        fn stop_bits(&self) -> Result<StopBits> {
            unimplemented!()
        }

        fn timeout(&self) -> core::time::Duration {
            unimplemented!()
        }

        fn set_baud_rate(&mut self, baud_rate: u32) -> Result<()> {
            unimplemented!()
        }

        fn set_data_bits(&mut self, data_bits: DataBits) -> Result<()> {
            unimplemented!()
        }

        fn set_flow_control(&mut self, flow_control: FlowControl) -> Result<()> {
            unimplemented!()
        }

        fn set_parity(&mut self, parity: Parity) -> Result<()> {
            unimplemented!()
        }

        fn set_stop_bits(&mut self, stop_bits: StopBits) -> Result<()> {
            unimplemented!()
        }

        fn set_timeout(&mut self, timeout: core::time::Duration) -> Result<()> {
            unimplemented!()
        }

        fn write_request_to_send(&mut self, level: bool) -> Result<()> {
            unimplemented!()
        }

        fn write_data_terminal_ready(&mut self, level: bool) -> Result<()> {
            unimplemented!()
        }

        fn read_clear_to_send(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_data_set_ready(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_ring_indicator(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_carrier_detect(&mut self) -> Result<bool> {
            unimplemented!()
        }
        fn bytes_to_write(&self) -> Result<u32> {
            unimplemented!()
        }

        fn clear(&self, buffer_to_clear: ClearBuffer) -> Result<()> {
            unimplemented!()
        }

        fn try_clone(&self) -> Result<Box<dyn SerialPort>> {
            unimplemented!()
        }

        fn set_break(&self) -> Result<()> {
            unimplemented!()
        }

        fn clear_break(&self) -> Result<()> {
            unimplemented!()
        }
    }
}
