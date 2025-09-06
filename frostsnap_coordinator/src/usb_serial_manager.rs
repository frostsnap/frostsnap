// USB CDC vid and pid
const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;

use crate::PortOpenError;
use crate::{FramedSerialPort, Serial};
use anyhow::anyhow;
use frostsnap_comms::{CommsMisc, ReceiveSerial};
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorUpgradeMessage, Destination, DeviceSendBody, Sha256Digest,
    FIRMWARE_IMAGE_SIZE, FIRMWARE_NEXT_CHUNK_READY_SIGNAL, FIRMWARE_UPGRADE_CHUNK_LEN,
};
use frostsnap_comms::{CoordinatorSendMessage, MAGIC_BYTES_PERIOD};
use frostsnap_core::message::DeviceToCoordinatorMessage;
use frostsnap_core::{sha2, DeviceId, Gist};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;
use tracing::{event, span, Level};

/// Manages the communication between coordinator and USB serial device ports given Some `S` serial
/// system API.
pub struct UsbSerialManager {
    serial_impl: Box<dyn Serial>,
    /// Matches VID and PID
    connected: HashSet<String>,
    /// Initial state
    pending: HashSet<String>,
    /// After opening port and awaiting magic bytes
    awaiting_magic: HashMap<String, AwaitingMagic>,
    /// Read magic magic bytes
    ready: HashMap<String, FramedSerialPort>,
    /// ports that seems to be busy
    ignored: HashSet<String>,
    /// Devices who Announce'd, mappings to port serial numbers
    device_ports: HashMap<DeviceId, DevicePort>,
    /// Reverse lookup from ports to devices (daisy chaining)
    reverse_device_ports: HashMap<String, Vec<DeviceId>>,
    /// Devices we sent registration ACK to
    registered_devices: BTreeSet<DeviceId>,
    /// Device labels
    device_names: HashMap<DeviceId, String>,
    /// Messages to devices waiting to be sent
    port_outbox: std::sync::mpsc::Receiver<CoordinatorSendMessage>,
    /// sometimes we need to put things in the outbox internally
    outbox_sender: std::sync::mpsc::Sender<CoordinatorSendMessage>,
    /// The firmware binary provided to devices who are doing an upgrade
    firmware_bin: Option<FirmwareBin>,
}

pub struct DevicePort {
    port: String,
    firmware_digest: Sha256Digest,
}

const COORDINATOR_MAGIC_BYTES_PERDIOD: std::time::Duration =
    std::time::Duration::from_millis(MAGIC_BYTES_PERIOD);

struct AwaitingMagic {
    port: FramedSerialPort,
    last_wrote_magic_bytes: Option<std::time::Instant>,
}

impl UsbSerialManager {
    /// Returns self and a `UsbSender` which can be used to queue messages
    pub fn new(serial_impl: Box<dyn Serial>, firmware_bin: Option<FirmwareBin>) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        Self {
            serial_impl,
            connected: Default::default(),
            pending: Default::default(),
            awaiting_magic: Default::default(),
            ready: Default::default(),
            ignored: Default::default(),
            device_ports: Default::default(),
            reverse_device_ports: Default::default(),
            registered_devices: Default::default(),
            device_names: Default::default(),
            port_outbox: receiver,
            outbox_sender: sender,
            firmware_bin,
        }
    }

    pub fn usb_sender(&self) -> UsbSender {
        UsbSender {
            sender: self.outbox_sender.clone(),
        }
    }

    fn disconnect(&mut self, port: &str, changes: &mut Vec<DeviceChange>) {
        event!(Level::INFO, port = port, "disconnecting port");
        self.connected.remove(port);
        self.pending.remove(port);
        self.awaiting_magic.remove(port);
        self.ready.remove(port);
        self.ignored.remove(port);
        if let Some(device_ids) = self.reverse_device_ports.remove(port) {
            for device_id in device_ids {
                if self.device_ports.remove(&device_id).is_some() {
                    changes.push(DeviceChange::Disconnected { id: device_id });
                }
                self.registered_devices.remove(&device_id);
                event!(
                    Level::DEBUG,
                    port = port,
                    device_id = device_id.to_string(),
                    "removing device because of disconnected port"
                )
            }
        }
    }

    pub fn active_ports(&self) -> HashSet<String> {
        self.registered_devices
            .iter()
            .filter_map(|device_id| {
                self.device_ports
                    .get(device_id)
                    .map(|device_port| &device_port.port)
            })
            .cloned()
            .collect::<HashSet<_>>()
    }

    pub fn poll_ports(&mut self) -> Vec<DeviceChange> {
        let span = span!(Level::DEBUG, "poll_ports");
        let _enter = span.enter();
        let mut device_changes = vec![];

        let connected_now: HashSet<String> = self
            .serial_impl
            .available_ports()
            .into_iter()
            .filter(|desc| desc.vid == USB_VID && desc.pid == USB_PID)
            .map(|desc| desc.id)
            .collect();

        let newly_connected_ports = connected_now
            .difference(&self.connected)
            .cloned()
            .collect::<Vec<_>>();
        for port in newly_connected_ports {
            event!(Level::INFO, port = port, "USB port connected");
            self.connected.insert(port.clone());
            self.pending.insert(port.clone());
        }

        let disconnected_ports = self
            .connected
            .difference(&connected_now)
            .cloned()
            .collect::<Vec<_>>();
        for port in disconnected_ports {
            event!(
                Level::DEBUG,
                port = port.to_string(),
                "USB port disconnected"
            );
            self.disconnect(&port, &mut device_changes);
        }

        for port_name in self.pending.drain().collect::<Vec<_>>() {
            let device_port = self
                .serial_impl
                .open_device_port(&port_name, frostsnap_comms::BAUDRATE)
                .map(FramedSerialPort::new);
            match device_port {
                Err(e) => match e {
                    PortOpenError::DeviceBusy => {
                        if !self.ignored.contains(&port_name) {
                            event!(
                                Level::ERROR,
                                port = port_name,
                                "Could not open port because it's being used by another process"
                            );
                            self.ignored.insert(port_name.clone());
                        }
                    }
                    PortOpenError::Other(e) => {
                        event!(
                            Level::ERROR,
                            port = port_name,
                            error = e.to_string(),
                            "Failed to open port"
                        );
                        self.pending.insert(port_name);
                    }
                },
                Ok(device_port) => {
                    event!(Level::DEBUG, port = port_name, "Opened port");
                    self.awaiting_magic.insert(
                        port_name.clone(),
                        AwaitingMagic {
                            port: device_port,
                            last_wrote_magic_bytes: None,
                        },
                    );
                }
            }
        }

        for (port_name, mut awaiting_magic) in self.awaiting_magic.drain().collect::<Vec<_>>() {
            let device_port = &mut awaiting_magic.port;
            match device_port.read_for_magic_bytes() {
                Ok(Some(supported_features)) => {
                    event!(Level::DEBUG, port = port_name, "Read magic bytes");
                    device_port.set_conch_enabled(supported_features.conch_enabled);
                    self.ready.insert(port_name, awaiting_magic.port);
                }
                Ok(None) => {
                    let time_since_last_wrote_magic = awaiting_magic
                        .last_wrote_magic_bytes
                        .as_ref()
                        .map(std::time::Instant::elapsed)
                        .unwrap_or(std::time::Duration::MAX);

                    if time_since_last_wrote_magic < COORDINATOR_MAGIC_BYTES_PERDIOD {
                        self.awaiting_magic.insert(port_name, awaiting_magic);
                        continue;
                    }

                    match device_port.write_magic_bytes() {
                        Ok(_) => {
                            event!(Level::DEBUG, port = port_name, "Wrote magic bytes");
                            awaiting_magic.last_wrote_magic_bytes = Some(std::time::Instant::now());
                            // we still need to read them so go again
                            self.awaiting_magic.insert(port_name, awaiting_magic);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                port = port_name,
                                e = e.to_string(),
                                "Failed to write magic bytes"
                            );
                            self.disconnect(&port_name, &mut device_changes);
                        }
                    }
                }
                Err(e) => {
                    event!(
                        Level::DEBUG,
                        port = port_name,
                        e = e.to_string(),
                        "failed to read magic bytes"
                    );
                    self.disconnect(&port_name, &mut device_changes);
                }
            }
        }

        // Read all messages from ready devices
        for port_name in self.ready.keys().cloned().collect::<Vec<_>>() {
            let frame = {
                let device_port = self.ready.get_mut(&port_name).expect("must exist");
                match device_port.try_read_message() {
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            port = port_name,
                            error = e.to_string(),
                            "failed to read message from port"
                        );
                        self.disconnect(&port_name, &mut device_changes);
                        continue;
                    }
                    Ok(None) => continue,
                    Ok(Some(message)) => message,
                }
            };

            match frame {
                ReceiveSerial::MagicBytes(_) => {
                    event!(Level::ERROR, port = port_name, "Unexpected magic bytes");
                    self.disconnect(&port_name, &mut device_changes);
                }
                ReceiveSerial::Message(message) => {
                    match message.body.decode() {
                        Err(e) => {
                            event!(
                                Level::WARN,
                                from = message.from.to_string(),
                                error = e.to_string(),
                                "failed to decode encapsulated message - ignoring"
                            );
                        }
                        Ok(decoded) => {
                            event!(
                                Level::DEBUG,
                                from = message.from.to_string(),
                                port = port_name,
                                gist = decoded.gist(),
                                "decoded message"
                            );

                            match decoded {
                                DeviceSendBody::NeedName => device_changes
                                    .push(DeviceChange::NeedsName { id: message.from }),
                                DeviceSendBody::DisconnectDownstream => {
                                    if let Some(device_list) =
                                        self.reverse_device_ports.get_mut(&port_name)
                                    {
                                        if let Some((i, _)) = device_list
                                            .iter()
                                            .enumerate()
                                            .find(|(_, device_id)| **device_id == message.from)
                                        {
                                            let index_of_disconnection = i + 1;
                                            while device_list.len() > index_of_disconnection {
                                                let device_id = device_list.pop().unwrap();
                                                self.device_ports.remove(&device_id);
                                                self.registered_devices.remove(&device_id);
                                                device_changes.push(DeviceChange::Disconnected {
                                                    id: device_id,
                                                });
                                            }
                                        }
                                    }
                                }
                                DeviceSendBody::SetName { name } => {
                                    let existing_name = self.device_names.get(&message.from);
                                    if existing_name != Some(&name) {
                                        device_changes.push(DeviceChange::NameChange {
                                            id: message.from,
                                            name,
                                        });
                                    }
                                }
                                DeviceSendBody::Announce { firmware_digest } => {
                                    self.handle_announce(
                                        &port_name,
                                        message.from,
                                        firmware_digest,
                                        &mut device_changes,
                                    );
                                }
                                DeviceSendBody::Debug { message: _ } => {
                                    // XXX: We don't need to debug log this because we already debug log the gist of every message
                                    // event!(
                                    //     Level::DEBUG,
                                    //     port = port_name,
                                    //     from = message.from.to_string(),
                                    //     name = self
                                    //         .device_names
                                    //         .get(&message.from)
                                    //         .cloned()
                                    //         .unwrap_or("<unknown>".into()),
                                    //     dbg_message
                                    // );
                                }
                                DeviceSendBody::Core(core_msg) => {
                                    device_changes.push(DeviceChange::AppMessage(AppMessage {
                                        from: message.from,
                                        body: AppMessageBody::Core(Box::new(core_msg)),
                                    }));
                                }
                                DeviceSendBody::_LegacyAckUpgradeMode => {
                                    device_changes.push(DeviceChange::AppMessage(AppMessage {
                                        from: message.from,
                                        body: AppMessageBody::Misc(CommsMisc::AckUpgradeMode),
                                    }))
                                }
                                DeviceSendBody::Misc(inner) => {
                                    device_changes.push(DeviceChange::AppMessage(AppMessage {
                                        from: message.from,
                                        body: AppMessageBody::Misc(inner),
                                    }))
                                }
                            }
                        }
                    }
                }
                ReceiveSerial::Reset => {
                    event!(Level::DEBUG, port = port_name, "Read reset downstream!");
                    self.disconnect(&port_name, &mut device_changes);
                }
                _ => { /* unused */ }
            }
        }

        for device_id in self.device_ports.keys() {
            if self.registered_devices.contains(device_id) {
                continue;
            }

            if let Some(device_label) = self.device_names.get(device_id) {
                event!(
                    Level::INFO,
                    device_id = device_id.to_string(),
                    "Registered device"
                );
                self.registered_devices.insert(*device_id);
                device_changes.push(DeviceChange::Registered {
                    id: *device_id,
                    name: device_label.to_string(),
                });
            }
        }

        while let Ok(mut send) = self.port_outbox.try_recv() {
            let mut ports_to_send_on = HashSet::new();
            let wire_destinations = match &mut send.target_destinations {
                Destination::All => {
                    ports_to_send_on.extend(
                        self.device_ports
                            .values()
                            .map(|device_port| &device_port.port)
                            .cloned(),
                    );
                    Destination::All
                }
                Destination::Particular(devices) => {
                    // You might be wondering why we bother to narrow down the wire destinations to
                    // those devices that are actually available. There is no good reason for this
                    // atm but it used to be necessary and it's nice to have only the devices that
                    // were actually visible to the coordinator on a particular port receive
                    // messages for sanity.
                    let mut destinations_available_now = BTreeSet::default();
                    devices.retain(|destination| match self.device_ports.get(destination) {
                        Some(device_port) => {
                            ports_to_send_on.insert(device_port.port.clone());
                            destinations_available_now.insert(*destination);
                            false
                        }
                        None => true,
                    });

                    if !devices.is_empty() {
                        event!(
                            Level::DEBUG,
                            kind = send.gist(),
                            "message not sent to all intended recipients"
                        );
                    }

                    Destination::Particular(destinations_available_now)
                }
            };

            let mut message = send.clone();
            message.target_destinations = wire_destinations;
            let dest_span = tracing::span!(
                Level::DEBUG,
                "",
                destinations = message.target_destinations.gist()
            );
            let _dest_enter = dest_span.enter();

            let gist = message.gist();

            for port_name in ports_to_send_on {
                let span =
                    tracing::span!(Level::INFO, "send on port", port = port_name, gist = gist);
                let _enter = span.enter();
                let port = match self.ready.get_mut(&port_name) {
                    Some(port) => port,
                    None => {
                        event!(
                            Level::DEBUG,
                            "not sending message because port was disconnected"
                        );
                        continue;
                    }
                };
                event!(Level::DEBUG, message = message.gist(), "queueing message");
                port.queue_send(message.clone());
            }
        }

        // poll the ports to send any messages we just queued (or queued from earlier!).
        // This is a separate step since we only send messages if we have the conch.
        for port_name in self.ready.keys().cloned().collect::<Vec<_>>() {
            let port = self.ready.get_mut(&port_name).expect("must exist");
            match port.poll_send() {
                Err(e) => {
                    event!(
                        Level::ERROR,
                        port = port_name,
                        error = e.to_string(),
                        "Failed to poll sending",
                    );
                    self.disconnect(&port_name, &mut device_changes);
                }
                Ok(_) => { /* nothing to do */ }
            }
        }

        device_changes
    }

    fn handle_announce(
        &mut self,
        port_name: &str,
        from: DeviceId,
        firmware_digest: Sha256Digest,
        device_changes: &mut Vec<DeviceChange>,
    ) {
        match self.device_ports.insert(
            from,
            DevicePort {
                port: port_name.to_string(),
                firmware_digest,
            },
        ) {
            Some(old_port_name) => {
                self.reverse_device_ports
                    .entry(old_port_name.port)
                    .or_default()
                    .retain(|device_id| *device_id != from);
            }
            None => device_changes.push(DeviceChange::Connected {
                id: from,
                firmware_digest,
                latest_firmware_digest: self
                    .firmware_bin
                    .map(|mut firmware_bin| firmware_bin.cached_digest()),
            }),
        }

        self.outbox_sender
            .send(CoordinatorSendMessage::to(
                from,
                CoordinatorSendBody::AnnounceAck,
            ))
            .unwrap();

        self.reverse_device_ports
            .entry(port_name.to_string())
            .or_default()
            .push(from);

        event!(
            Level::DEBUG,
            port = port_name,
            id = from.to_string(),
            "Announced!"
        );
    }

    pub fn registered_devices(&self) -> &BTreeSet<DeviceId> {
        &self.registered_devices
    }

    pub fn accept_device_name(&mut self, id: DeviceId, name: String) {
        self.device_names.insert(id, name);
    }

    pub fn serial_impl(&self) -> &dyn Serial {
        &*self.serial_impl
    }

    pub fn serial_impl_mut(&mut self) -> &mut dyn Serial {
        &mut *self.serial_impl
    }

    pub fn devices_by_ports(&self) -> &HashMap<String, Vec<DeviceId>> {
        &self.reverse_device_ports
    }

    /// The firmware digest the device has declared it has
    pub fn firmware_digest_for_device(&self, device_id: DeviceId) -> Option<Sha256Digest> {
        self.device_ports
            .get(&device_id)
            .map(|device_port| device_port.firmware_digest)
    }

    pub fn upgrade_bin(&self) -> Option<FirmwareBin> {
        self.firmware_bin
    }

    pub fn run_firmware_upgrade(
        &mut self,
    ) -> anyhow::Result<impl Iterator<Item = anyhow::Result<f32>> + '_> {
        let firmware_bin = self.firmware_bin.ok_or(anyhow!(
            "App wasn't compiled with BUNDLE_FIRMWARE=1 so it can't do firmware upgrades"
        ))?;
        let n_chunks = firmware_bin.size().div_ceil(FIRMWARE_UPGRADE_CHUNK_LEN);
        let total_chunks = n_chunks * self.ready.len() as u32;

        let mut iters = vec![];

        for (port_index, (port, io)) in self.ready.iter_mut().enumerate() {
            let res = io.raw_send(ReceiveSerial::Message(CoordinatorSendMessage {
                target_destinations: Destination::All,
                message_body: CoordinatorSendBody::Upgrade(
                    CoordinatorUpgradeMessage::EnterUpgradeMode,
                )
                .into(),
            }));

            // give some time for devices to forward things and enter upgrade mode
            std::thread::sleep(Duration::from_millis(100));

            if let Err(e) = res {
                event!(
                    Level::ERROR,
                    port = port,
                    error = e.to_string(),
                    "unable to send firmware upgrade initialiazation message"
                );
                continue;
            }

            io.wait_for_conch()?;

            event!(Level::INFO, port = port, "starting writing firmware");
            let mut chunks = firmware_bin
                .bin
                .chunks(FIRMWARE_UPGRADE_CHUNK_LEN as usize)
                .enumerate();

            iters.push(core::iter::from_fn(move || {
                let (i, chunk) = chunks.next()?;
                if let Err(e) = io.raw_write(chunk) {
                    event!(
                        Level::ERROR,
                        port = port,
                        error = e.to_string(),
                        "writing firmware failed"
                    );
                    return Some(Err(e.into()));
                }
                let mut byte = [0u8; 1];

                match io.raw_read(&mut byte[..]) {
                    Ok(_) => {
                        if byte[0] != FIRMWARE_NEXT_CHUNK_READY_SIGNAL {
                            event!(
                                Level::DEBUG,
                                byte = byte[0].to_string(),
                                "downstream device wrote invalid signal byte"
                            );
                        }
                    }
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            port = port,
                            error = e.to_string(),
                            "reading firmware progress signaling byte failed"
                        );
                        return Some(Err(e.into()));
                    }
                }

                Some(Ok(
                    ((port_index as u32 * n_chunks) + i as u32) as f32 / (total_chunks - 1) as f32
                ))
            }));
        }

        Ok(iters.into_iter().flatten())
    }
}

#[derive(Clone)]
pub struct UsbSender {
    sender: std::sync::mpsc::Sender<CoordinatorSendMessage>,
}

impl UsbSender {
    pub fn send_cancel_all(&self) {
        self.sender
            .send(CoordinatorSendMessage {
                target_destinations: frostsnap_comms::Destination::All,
                message_body: frostsnap_comms::CoordinatorSendBody::Cancel,
            })
            .expect("receiver exists");
    }

    pub fn send_cancel(&self, device_id: DeviceId) {
        self.sender
            .send(CoordinatorSendMessage::to(
                device_id,
                frostsnap_comms::CoordinatorSendBody::Cancel,
            ))
            .expect("receiver exists");
    }

    pub fn update_name_preview(&self, device_id: DeviceId, name: &str) {
        self.sender
            .send(CoordinatorSendMessage::to(
                device_id,
                CoordinatorSendBody::Naming(frostsnap_comms::NameCommand::Preview(name.into())),
            ))
            .expect("receiver exists");
    }

    pub fn finish_naming(&self, device_id: DeviceId, name: &str) {
        event!(
            Level::INFO,
            name = name,
            device_id = device_id.to_string(),
            "Named device"
        );
        self.sender
            .send(CoordinatorSendMessage::to(
                device_id,
                CoordinatorSendBody::Naming(frostsnap_comms::NameCommand::Prompt(name.into())),
            ))
            .expect("receiver exists");
    }

    pub fn send(&self, message: CoordinatorSendMessage) {
        self.sender.send(message).expect("receiver exists")
    }

    pub fn wipe_device_data(&self, device_id: DeviceId) {
        event!(
            Level::INFO,
            device_id = device_id.to_string(),
            "Wiping device"
        );
        self.sender
            .send(CoordinatorSendMessage::to(
                device_id,
                CoordinatorSendBody::DataWipe,
            ))
            .expect("receiver exists");
    }

    pub fn wipe_all(&self) {
        self.sender
            .send(CoordinatorSendMessage {
                target_destinations: Destination::All,
                message_body: CoordinatorSendBody::DataWipe,
            })
            .expect("receiver exists");
    }

    pub fn send_from_core(
        &self,
        messages: impl IntoIterator<Item = frostsnap_core::coordinator::CoordinatorSend>,
    ) {
        for message in messages {
            match CoordinatorSendMessage::try_from(message) {
                Ok(m) => {
                    self.sender.send(m).expect("receiver exists");
                }
                Err(e) => {
                    event!(
                        Level::WARN,
                        error = e.to_string(),
                        "tried to send a non-usb message over usb"
                    );
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum DeviceChange {
    Connected {
        id: DeviceId,
        firmware_digest: Sha256Digest,
        latest_firmware_digest: Option<Sha256Digest>,
    },
    NeedsName {
        id: DeviceId,
    },
    NameChange {
        id: DeviceId,
        name: String,
    },
    Registered {
        id: DeviceId,
        name: String,
    },
    Disconnected {
        id: DeviceId,
    },
    AppMessage(AppMessage),
}

#[derive(Debug, Clone)]
pub struct AppMessage {
    pub from: DeviceId,
    pub body: AppMessageBody,
}

#[derive(Debug, Clone)]
pub enum AppMessageBody {
    Core(Box<DeviceToCoordinatorMessage>),
    Misc(CommsMisc),
}

#[derive(Clone, Copy)]
pub struct FirmwareBin {
    bin: &'static [u8],
    digest_cache: Option<Sha256Digest>,
}

impl FirmwareBin {
    pub const fn is_stub(&self) -> bool {
        self.bin.is_empty()
    }

    pub const fn new(bin: &'static [u8]) -> Self {
        Self {
            bin,
            digest_cache: None,
        }
    }

    pub fn num_chunks(&self) -> u32 {
        (self.bin.len() as u32).div_ceil(FIRMWARE_UPGRADE_CHUNK_LEN)
    }

    pub fn size(&self) -> u32 {
        self.bin.len() as u32
    }

    pub fn cached_digest(&mut self) -> Sha256Digest {
        let digest_cache = self.digest_cache.take();
        let digest = digest_cache.unwrap_or_else(|| self.digest());
        self.digest_cache = Some(digest);
        digest
    }

    pub fn digest(&self) -> Sha256Digest {
        use frostsnap_core::sha2::digest::Digest;
        let mut state = sha2::Sha256::default();
        state.update(self.bin);
        Sha256Digest(state.finalize().into())
    }

    #[allow(dead_code)]
    fn padded_digest(&self) -> Sha256Digest {
        use frostsnap_core::sha2::digest::Digest;
        let mut state = sha2::Sha256::default();
        state.update(self.bin);
        let mut len = self.bin.len();
        while len < FIRMWARE_IMAGE_SIZE as usize {
            len += 1;
            state.update([0xff]);
        }
        Sha256Digest(state.finalize().into())
    }
}
