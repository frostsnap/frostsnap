// USB CDC vid and pid
const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;

use crate::firmware::{FirmwareVersion, ValidatedFirmwareBin};
use crate::frostsnap_persist::Attestation;
use crate::genuine_check::{GenuineCheck, GenuineResponse, GenuineStatus};
use crate::PortOpenError;
use crate::{FramedSerialPort, Serial};
use anyhow::anyhow;
use frostsnap_comms::DeviceName;
use frostsnap_comms::{CommsMisc, ReceiveSerial};
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorUpgradeMessage, Destination, DeviceSendBody, Sha256Digest,
    FIRMWARE_NEXT_CHUNK_READY_SIGNAL, FIRMWARE_UPGRADE_CHUNK_LEN,
};
use frostsnap_comms::{CoordinatorSendMessage, MAGIC_BYTES_PERIOD};
use frostsnap_core::message::DeviceToCoordinatorMessage;
use frostsnap_core::{DeviceId, Gist};
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
    firmware_bin: Option<ValidatedFirmwareBin>,
    /// `None` in a build with no factory key.
    genuine_check: Option<GenuineCheck>,
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
    pub fn new(serial_impl: Box<dyn Serial>) -> Self {
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
            firmware_bin: None,
            genuine_check: None,
        }
    }

    pub fn with_firmware_bin(mut self, firmware_bin: ValidatedFirmwareBin) -> Self {
        self.firmware_bin = Some(firmware_bin);
        self
    }

    pub fn with_genuine_check(mut self, genuine_check: GenuineCheck) -> Self {
        self.genuine_check = Some(genuine_check);
        self
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
                self.forget_device(device_id);
                event!(
                    Level::DEBUG,
                    port = port,
                    device_id = device_id.to_string(),
                    "removing device because of disconnected port"
                )
            }
        }
    }

    /// Every path that removes a device must call this.
    fn forget_device(&mut self, device_id: DeviceId) {
        self.registered_devices.remove(&device_id);
        if let Some(genuine_check) = &mut self.genuine_check {
            genuine_check.disconnected(device_id);
        }
    }

    fn recv_genuine(&mut self, from: DeviceId, response: GenuineResponse) {
        if let Some(message) = self
            .genuine_check
            .as_mut()
            .and_then(|genuine_check| genuine_check.recv(from, response))
        {
            self.outbox_sender.send(message).unwrap();
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
                    PortOpenError::PermissionDenied => {
                        if !self.ignored.contains(&port_name) {
                            event!(
                                Level::WARN,
                                port = port_name,
                                "Could not open port: permission denied, retrying (udev rules may not be installed)"
                            );
                            self.ignored.insert(port_name.clone());
                        }
                        // Always retry — the port often appears before udev has
                        // finished applying permission rules.
                        self.pending.insert(port_name);
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
                                            let mut disconnected = vec![];
                                            while device_list.len() > index_of_disconnection {
                                                let device_id = device_list.pop().unwrap();
                                                self.device_ports.remove(&device_id);
                                                disconnected.push(device_id);
                                                device_changes.push(DeviceChange::Disconnected {
                                                    id: device_id,
                                                });
                                            }
                                            for device_id in disconnected {
                                                self.forget_device(device_id);
                                            }
                                        }
                                    }
                                }
                                DeviceSendBody::SetName { name } => {
                                    let name_string = name.to_string();
                                    let existing_name = self.device_names.get(&message.from);
                                    if existing_name != Some(&name_string) {
                                        device_changes.push(DeviceChange::NameChange {
                                            id: message.from,
                                            name: name_string,
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
                                DeviceSendBody::_LegacySignedChallenge { .. } => {}
                                DeviceSendBody::GenuineAttestation {
                                    certificate,
                                    ds_signature,
                                } => self.recv_genuine(
                                    message.from,
                                    GenuineResponse::Attestation(Box::new(Attestation {
                                        certificate: *certificate,
                                        ds_signature,
                                    })),
                                ),
                                DeviceSendBody::GenuineIdentityProof { signature } => self
                                    .recv_genuine(
                                        message.from,
                                        GenuineResponse::IdentityProof(signature),
                                    ),
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
                port.queue_send(message.clone().into());
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

        if let Some(genuine_check) = &mut self.genuine_check {
            device_changes.extend(
                genuine_check
                    .take_changes()
                    .into_iter()
                    .map(|(id, status)| DeviceChange::Genuine { id, status }),
            );
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
                latest_firmware_digest: self.firmware_bin.map(|firmware_bin| firmware_bin.digest()),
            }),
        }

        self.outbox_sender
            .send(CoordinatorSendMessage::to(
                from,
                CoordinatorSendBody::AnnounceAck,
            ))
            .unwrap();

        if let Some(message) = self.genuine_check.as_mut().and_then(|genuine_check| {
            genuine_check.connected(from, port_name, FirmwareVersion::new(firmware_digest))
        }) {
            self.outbox_sender.send(message).unwrap();
        }

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

    pub fn upgrade_bin(&self) -> Option<ValidatedFirmwareBin> {
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
                .as_bytes()
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

    pub fn update_name_preview(&self, device_id: DeviceId, name: DeviceName) {
        self.sender
            .send(CoordinatorSendMessage::to(
                device_id,
                CoordinatorSendBody::Naming(frostsnap_comms::NameCommand::Preview(name)),
            ))
            .expect("receiver exists");
    }

    pub fn send(&self, message: CoordinatorSendMessage) {
        self.sender.send(message).expect("receiver exists")
    }

    pub fn erase_device_data(&self, device_id: DeviceId) {
        event!(
            Level::INFO,
            device_id = device_id.to_string(),
            "Wiping device"
        );
        self.sender
            .send(CoordinatorSendMessage::to(
                device_id,
                CoordinatorSendBody::DataErase,
            ))
            .expect("receiver exists");
    }

    pub fn erase_all(&self) {
        self.sender
            .send(CoordinatorSendMessage {
                target_destinations: Destination::All,
                message_body: CoordinatorSendBody::DataErase,
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
    Genuine {
        id: DeviceId,
        status: GenuineStatus,
    },
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::frostsnap_persist::GenuineCerts;
    use crate::genuine_check::test::{supported, TestDevice, FACTORY};
    use crate::PortDesc;
    use frostsnap_comms::{DeviceSendMessage, Upstream};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    type Pipe = Arc<Mutex<VecDeque<u8>>>;

    /// Ports by name, each a (to coordinator, to device) pair of pipes.
    #[derive(Clone, Default)]
    struct FakeSerial(Arc<Mutex<HashMap<String, (Pipe, Pipe)>>>);

    impl Serial for FakeSerial {
        fn available_ports(&self) -> Vec<PortDesc> {
            self.0
                .lock()
                .unwrap()
                .keys()
                .map(|id| PortDesc {
                    id: id.clone(),
                    vid: USB_VID,
                    pid: USB_PID,
                })
                .collect()
        }

        fn open_device_port(&self, id: &str, _: u32) -> Result<crate::SerialPort, PortOpenError> {
            let (to_coordinator, to_device) = self
                .0
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or(PortOpenError::Other("unplugged".into()))?;
            Ok(Box::new(FakePort {
                rx: to_coordinator,
                tx: to_device,
            }))
        }
    }

    struct FakePort {
        rx: Pipe,
        tx: Pipe,
    }

    impl std::io::Read for FakePort {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut rx = self.rx.lock().unwrap();
            if rx.is_empty() {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
            let n = buf.len().min(rx.len());
            for (slot, byte) in buf.iter_mut().zip(rx.drain(..n)) {
                *slot = byte;
            }
            Ok(n)
        }
    }

    impl std::io::Write for FakePort {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.tx.lock().unwrap().extend(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl serialport::SerialPort for FakePort {
        fn name(&self) -> Option<String> {
            None
        }
        fn baud_rate(&self) -> serialport::Result<u32> {
            Ok(frostsnap_comms::BAUDRATE)
        }
        fn data_bits(&self) -> serialport::Result<serialport::DataBits> {
            Ok(serialport::DataBits::Eight)
        }
        fn flow_control(&self) -> serialport::Result<serialport::FlowControl> {
            Ok(serialport::FlowControl::None)
        }
        fn parity(&self) -> serialport::Result<serialport::Parity> {
            Ok(serialport::Parity::None)
        }
        fn stop_bits(&self) -> serialport::Result<serialport::StopBits> {
            Ok(serialport::StopBits::One)
        }
        fn timeout(&self) -> Duration {
            Duration::ZERO
        }
        fn set_baud_rate(&mut self, _: u32) -> serialport::Result<()> {
            Ok(())
        }
        fn set_data_bits(&mut self, _: serialport::DataBits) -> serialport::Result<()> {
            Ok(())
        }
        fn set_flow_control(&mut self, _: serialport::FlowControl) -> serialport::Result<()> {
            Ok(())
        }
        fn set_parity(&mut self, _: serialport::Parity) -> serialport::Result<()> {
            Ok(())
        }
        fn set_stop_bits(&mut self, _: serialport::StopBits) -> serialport::Result<()> {
            Ok(())
        }
        fn set_timeout(&mut self, _: Duration) -> serialport::Result<()> {
            Ok(())
        }
        fn write_request_to_send(&mut self, _: bool) -> serialport::Result<()> {
            Ok(())
        }
        fn write_data_terminal_ready(&mut self, _: bool) -> serialport::Result<()> {
            Ok(())
        }
        fn read_clear_to_send(&mut self) -> serialport::Result<bool> {
            Ok(true)
        }
        fn read_data_set_ready(&mut self) -> serialport::Result<bool> {
            Ok(true)
        }
        fn read_ring_indicator(&mut self) -> serialport::Result<bool> {
            Ok(false)
        }
        fn read_carrier_detect(&mut self) -> serialport::Result<bool> {
            Ok(true)
        }
        fn bytes_to_read(&self) -> serialport::Result<u32> {
            Ok(self.rx.lock().unwrap().len() as u32)
        }
        fn bytes_to_write(&self) -> serialport::Result<u32> {
            Ok(0)
        }
        fn clear(&self, _: serialport::ClearBuffer) -> serialport::Result<()> {
            Ok(())
        }
        fn try_clone(&self) -> serialport::Result<Box<dyn serialport::SerialPort>> {
            Ok(Box::new(FakePort {
                rx: self.rx.clone(),
                tx: self.tx.clone(),
            }))
        }
        fn set_break(&self) -> serialport::Result<()> {
            Ok(())
        }
        fn clear_break(&self) -> serialport::Result<()> {
            Ok(())
        }
    }

    struct FakeDevice {
        device: TestDevice,
        port: FramedSerialPort<Upstream>,
        announced: bool,
        answers_genuine_check: bool,
        received: Vec<CoordinatorSendBody>,
    }

    impl FakeDevice {
        fn plug_in(
            serial: &FakeSerial,
            port_name: &str,
            device: TestDevice,
            answers_genuine_check: bool,
        ) -> Self {
            let to_coordinator = Pipe::default();
            let to_device = Pipe::default();
            serial.0.lock().unwrap().insert(
                port_name.to_string(),
                (to_coordinator.clone(), to_device.clone()),
            );
            Self {
                device,
                port: FramedSerialPort::new(Box::new(FakePort {
                    rx: to_device,
                    tx: to_coordinator,
                })),
                announced: false,
                answers_genuine_check,
                received: vec![],
            }
        }

        fn send(&mut self, body: DeviceSendBody) {
            self.port.queue_send(DeviceSendMessage {
                from: self.device.id,
                body: body.into(),
            });
            self.port.poll_send().unwrap();
        }

        fn poll(&mut self) {
            if !self.announced {
                if self.port.read_for_magic_bytes().unwrap().is_some() {
                    self.port.write_magic_bytes().unwrap();
                    self.send(DeviceSendBody::Announce {
                        firmware_digest: supported().digest,
                    });
                    self.announced = true;
                }
                return;
            }
            while let Some(frame) = self.port.try_read_message().unwrap() {
                let ReceiveSerial::Message(message) = frame else {
                    continue;
                };
                let Some(body) = message.message_body.decode() else {
                    continue;
                };
                self.received.push(body.clone());
                let is_genuine_request = matches!(
                    body,
                    CoordinatorSendBody::RequestGenuineAttestation
                        | CoordinatorSendBody::GenuineIdentityChallenge(_)
                );
                if is_genuine_request && self.answers_genuine_check {
                    let answer = self
                        .device
                        .answer(&CoordinatorSendMessage::to(self.device.id, body));
                    self.send(match answer {
                        GenuineResponse::Attestation(attestation) => {
                            DeviceSendBody::GenuineAttestation {
                                certificate: Box::new(attestation.certificate),
                                ds_signature: attestation.ds_signature,
                            }
                        }
                        GenuineResponse::IdentityProof(signature) => {
                            DeviceSendBody::GenuineIdentityProof { signature }
                        }
                    });
                }
            }
        }
    }

    fn manager(serial: &FakeSerial) -> UsbSerialManager {
        UsbSerialManager::new(Box::new(serial.clone())).with_genuine_check(GenuineCheck::new(
            FACTORY.public_key(),
            Arc::new(Mutex::new(GenuineCerts::default())),
        ))
    }

    fn run(manager: &mut UsbSerialManager, device: &mut FakeDevice) -> Vec<DeviceChange> {
        let mut changes = vec![];
        for _ in 0..20 {
            changes.extend(manager.poll_ports());
            device.poll();
        }
        changes
    }

    fn genuine_statuses(changes: &[DeviceChange]) -> Vec<&GenuineStatus> {
        changes
            .iter()
            .filter_map(|change| match change {
                DeviceChange::Genuine { status, .. } => Some(status),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_device_that_never_answers_the_check_is_otherwise_served_normally() {
        let serial = FakeSerial::default();
        let mut manager = manager(&serial);
        let usb_sender = manager.usb_sender();
        let mut device = FakeDevice::plug_in(&serial, "a", TestDevice::new(40), false);

        let changes = run(&mut manager, &mut device);
        assert!(changes
            .iter()
            .any(|change| matches!(change, DeviceChange::Connected { .. })));
        assert!(device
            .received
            .iter()
            .any(|body| matches!(body, CoordinatorSendBody::RequestGenuineAttestation)));

        usb_sender.send(CoordinatorSendMessage::to(
            device.device.id,
            CoordinatorSendBody::DataErase,
        ));
        device.send(DeviceSendBody::NeedName);
        let changes = run(&mut manager, &mut device);

        assert!(matches!(
            device.received.last(),
            Some(CoordinatorSendBody::DataErase)
        ));
        assert!(changes
            .iter()
            .any(|change| matches!(change, DeviceChange::NeedsName { .. })));
        assert!(genuine_statuses(&changes).is_empty());
    }

    #[test]
    fn unplugging_returns_a_device_to_state_2_and_replugging_takes_one_query() {
        let serial = FakeSerial::default();
        let mut manager = manager(&serial);
        let mut device = FakeDevice::plug_in(&serial, "a", TestDevice::new(41), true);

        let changes = run(&mut manager, &mut device);
        assert!(matches!(
            genuine_statuses(&changes).last(),
            Some(GenuineStatus::Genuine { .. })
        ));

        serial.0.lock().unwrap().clear();
        let changes = run(&mut manager, &mut device);
        assert!(changes
            .iter()
            .any(|change| matches!(change, DeviceChange::Disconnected { .. })));

        let mut device = FakeDevice::plug_in(&serial, "a", TestDevice::new(41), true);
        let changes = run(&mut manager, &mut device);
        assert!(matches!(
            genuine_statuses(&changes).as_slice(),
            [
                GenuineStatus::Attested { .. },
                GenuineStatus::Genuine { .. }
            ]
        ));
        let genuine_requests: Vec<_> = device
            .received
            .iter()
            .filter(|body| {
                matches!(
                    body,
                    CoordinatorSendBody::RequestGenuineAttestation
                        | CoordinatorSendBody::GenuineIdentityChallenge(_)
                )
            })
            .collect();
        assert!(matches!(
            genuine_requests.as_slice(),
            [CoordinatorSendBody::GenuineIdentityChallenge(_)]
        ));
    }
}
