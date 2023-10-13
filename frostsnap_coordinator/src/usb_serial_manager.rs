// USB CDC vid and pid
const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;

use crate::PortOpenError;
use crate::{FramedSerialPort, Serial};
use frostsnap_comms::{CoordinatorSendBody, DeviceSendBody};
use frostsnap_comms::{CoordinatorSendMessage, MAGIC_BYTES_PERIOD};
use frostsnap_comms::{ReceiveSerial, Upstream};
use frostsnap_core::message::DeviceToCoordinatorMessage;
use frostsnap_core::{DeviceId, Gist};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
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
    device_ports: HashMap<DeviceId, String>,
    /// Reverse lookup from ports to devices (daisy chaining)
    reverse_device_ports: HashMap<String, Vec<DeviceId>>,
    /// Devices we sent registration ACK to
    registered_devices: BTreeSet<DeviceId>,
    /// Device labels
    device_labels: HashMap<DeviceId, String>,
    /// Messages to devices outbox
    port_outbox: Vec<CoordinatorSendMessage>,
}

const COORDINATOR_MAGIC_BYTES_PERDIOD: std::time::Duration =
    std::time::Duration::from_millis(MAGIC_BYTES_PERIOD);

struct AwaitingMagic {
    port: FramedSerialPort,
    last_wrote_magic_bytes: Option<std::time::Instant>,
}

impl UsbSerialManager {
    pub fn new(serial_impl: Box<dyn Serial>) -> Self {
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
            device_labels: Default::default(),
            port_outbox: Default::default(),
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

    pub fn queue_in_port_outbox(&mut self, send: CoordinatorSendMessage) {
        self.port_outbox.push(send);
    }

    pub fn active_ports(&self) -> HashSet<String> {
        self.registered_devices
            .iter()
            .filter_map(|device_id| self.device_ports.get(device_id))
            .cloned()
            .collect::<HashSet<_>>()
    }

    pub fn poll_ports(&mut self) -> PortChanges {
        let span = span!(Level::DEBUG, "poll_ports");
        let _enter = span.enter();
        let mut device_to_coord_msg = vec![];
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
            event!(Level::INFO, port = port.to_string(), "USB port connected");
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

        for serial_number in self.pending.drain().collect::<Vec<_>>() {
            let device_port = self
                .serial_impl
                .open_device_port(&serial_number, frostsnap_comms::BAUDRATE)
                .map(FramedSerialPort::new);
            match device_port {
                Err(e) => match e {
                    PortOpenError::DeviceBusy => {
                        if !self.ignored.contains(&serial_number) {
                            event!(
                                Level::ERROR,
                                port = serial_number,
                                "Could not open port because it's being used by another process"
                            );
                            self.ignored.insert(serial_number.clone());
                        }
                    }
                    PortOpenError::Other(e) => {
                        event!(
                            Level::ERROR,
                            port = serial_number,
                            error = e.to_string(),
                            "Failed to open port"
                        );
                        self.pending.insert(serial_number);
                    }
                },
                Ok(mut device_port) => {
                    event!(Level::DEBUG, port = serial_number, "Opened port");
                    match device_port.write_magic_bytes() {
                        Ok(_) => {
                            self.awaiting_magic.insert(
                                serial_number.clone(),
                                AwaitingMagic {
                                    port: device_port,
                                    last_wrote_magic_bytes: None,
                                },
                            );
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                port = serial_number,
                                e = e.to_string(),
                                "Failed to initialize port by writing magic bytes"
                            );
                            self.disconnect(&serial_number, &mut device_changes);
                        }
                    }
                }
            }
        }

        for (serial_number, mut awaiting_magic) in self.awaiting_magic.drain().collect::<Vec<_>>() {
            let device_port = &mut awaiting_magic.port;
            match device_port.read_for_magic_bytes() {
                Ok(true) => {
                    event!(Level::DEBUG, port = serial_number, "Read magic bytes");
                    self.ready.insert(serial_number, awaiting_magic.port);
                }
                Ok(false) => {
                    let time_since_last_wrote_magic = awaiting_magic
                        .last_wrote_magic_bytes
                        .as_ref()
                        .map(std::time::Instant::elapsed)
                        .unwrap_or(std::time::Duration::MAX);

                    if time_since_last_wrote_magic < COORDINATOR_MAGIC_BYTES_PERDIOD {
                        self.awaiting_magic.insert(serial_number, awaiting_magic);
                        continue;
                    }

                    match device_port.write_magic_bytes() {
                        Ok(_) => {
                            event!(Level::DEBUG, port = serial_number, "Wrote magic bytes");
                            awaiting_magic.last_wrote_magic_bytes = Some(std::time::Instant::now());
                            // we still need to read them so go again
                            self.awaiting_magic.insert(serial_number, awaiting_magic);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                port = serial_number,
                                e = e.to_string(),
                                "Failed to write magic bytes"
                            );
                            self.disconnect(&serial_number, &mut device_changes);
                        }
                    }
                }
                Err(e) => {
                    event!(
                        Level::DEBUG,
                        port = serial_number,
                        e = e.to_string(),
                        "failed to read magic bytes"
                    );
                    self.disconnect(&serial_number, &mut device_changes);
                }
            }
        }

        // Read all messages from ready devices
        for serial_number in self.ready.keys().cloned().collect::<Vec<_>>() {
            let decoded_message = {
                let device_port = self.ready.get_mut(&serial_number).expect("must exist");
                match device_port.try_read_message() {
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            port = serial_number,
                            error = e.to_string(),
                            "failed to read message from port"
                        );
                        self.disconnect(&serial_number, &mut device_changes);
                        continue;
                    }
                    Ok(None) => continue,
                    Ok(Some(message)) => message,
                }
            };

            event!(
                Level::DEBUG,
                port = serial_number,
                gist = decoded_message.gist(),
                "decoded message"
            );

            match decoded_message {
                ReceiveSerial::MagicBytes(_) => {
                    event!(Level::ERROR, port = serial_number, "Unexpected magic bytes");
                    self.disconnect(&serial_number, &mut device_changes);
                }
                ReceiveSerial::Message(message) => match message.body {
                    DeviceSendBody::NeedName => {
                        device_changes.push(DeviceChange::NeedsName { id: message.from })
                    }
                    DeviceSendBody::DisconnectDownstream => {
                        if let Some(device_list) = self.reverse_device_ports.get_mut(&serial_number)
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
                                    device_changes
                                        .push(DeviceChange::Disconnected { id: device_id });
                                }
                            }
                        }
                    }
                    DeviceSendBody::SetName { name } => {
                        match self.device_labels.get(&message.from) {
                            Some(existing_name) => {
                                if existing_name != &name {
                                    device_changes.push(DeviceChange::Renamed {
                                        id: message.from,
                                        old_name: existing_name.into(),
                                        new_name: name,
                                    });
                                }
                            }
                            None => {
                                self.device_labels.insert(message.from, name);
                            }
                        }
                    }
                    DeviceSendBody::Announce => {
                        match self
                            .device_ports
                            .insert(message.from, serial_number.clone())
                        {
                            Some(old_serial_number) => {
                                self.reverse_device_ports
                                    .entry(old_serial_number)
                                    .or_default()
                                    .retain(|device_id| *device_id != message.from);
                            }
                            None => device_changes.push(DeviceChange::Added { id: message.from }),
                        }

                        self.port_outbox.push(CoordinatorSendMessage {
                            message_body: CoordinatorSendBody::AnnounceAck {},
                            target_destinations: BTreeSet::from([message.from]),
                        });

                        self.reverse_device_ports
                            .entry(serial_number.clone())
                            .or_default()
                            .push(message.from);

                        event!(
                            Level::DEBUG,
                            port = serial_number,
                            id = message.from.to_string(),
                            "Announced!"
                        );
                    }
                    DeviceSendBody::Debug {
                        message: dbg_message,
                    } => {
                        event!(
                            Level::DEBUG,
                            port = serial_number,
                            from = message.from.to_string(),
                            name = self
                                .device_labels
                                .get(&message.from)
                                .cloned()
                                .unwrap_or("<unknown>".into()),
                            dbg_message
                        );
                    }
                    DeviceSendBody::Core(core_msg) => {
                        device_to_coord_msg.push((message.from, core_msg))
                    }
                },
            }
        }

        let mut outbox = core::mem::take(&mut self.port_outbox);

        for device_id in self.device_ports.keys() {
            if self.registered_devices.contains(device_id) {
                continue;
            }

            if let Some(device_label) = self.device_labels.get(device_id) {
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

        outbox.retain_mut(|send| {
            let mut ports_to_send_on = HashSet::new();
            let mut wire_message = send.clone();
            wire_message.target_destinations.clear();

            send.target_destinations.retain(|destination| {
                match self.device_ports.get(destination) {
                    Some(port) => {
                        ports_to_send_on.insert(port.clone());
                        wire_message.target_destinations.insert(*destination);
                        false
                    }
                    None => true,
                }
            });

            let wire_message = ReceiveSerial::<Upstream>::Message(wire_message);
            let gist = wire_message.gist();

            for serial_number in ports_to_send_on {
                let span = tracing::span!(
                    Level::ERROR,
                    "send on port",
                    port = serial_number,
                    gist = gist
                );
                let _enter = span.enter();
                let port = match self.ready.get_mut(&serial_number) {
                    Some(port) => port,
                    None => {
                        event!(
                            Level::DEBUG,
                            "not sending message because port was disconnected"
                        );
                        continue;
                    }
                };
                match port.send_message(&wire_message) {
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            error = e.to_string(),
                            "Failed to send message",
                        );
                        self.disconnect(&serial_number, &mut device_changes);
                    }
                    Ok(_) => {
                        event!(Level::DEBUG, "Sent message",);
                    }
                }
            }

            !send.target_destinations.is_empty()
        });

        self.port_outbox = outbox;

        PortChanges {
            device_changes,
            new_messages: device_to_coord_msg,
        }
    }

    pub fn device_labels_mut(&mut self) -> &mut HashMap<DeviceId, String> {
        &mut self.device_labels
    }

    pub fn unnamed_devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.announced_devices()
            .filter(|(_, label)| label.is_none())
            .map(|(device, _)| device)
    }
    pub fn device_labels(&self) -> &HashMap<DeviceId, String> {
        &self.device_labels
    }

    pub fn announced_devices(&self) -> impl Iterator<Item = (DeviceId, Option<String>)> + '_ {
        self.device_ports
            .keys()
            .map(|device| (*device, self.device_labels.get(device).cloned()))
    }

    pub fn registered_devices(&self) -> &BTreeSet<DeviceId> {
        &self.registered_devices
    }

    pub fn update_name_preview(&mut self, device_id: DeviceId, name: &str) {
        // repalce name preview messages rather than spamming
        if matches!(
            self.port_outbox.last(),
            Some(CoordinatorSendMessage {
                message_body: CoordinatorSendBody::Naming(frostsnap_comms::NameCommand::Preview(_)),
                ..
            })
        ) {
            self.port_outbox.pop();
        }
        self.port_outbox.push(CoordinatorSendMessage {
            target_destinations: [device_id].into(),
            message_body: CoordinatorSendBody::Naming(frostsnap_comms::NameCommand::Preview(
                name.into(),
            )),
        });
    }

    pub fn finish_naming(&mut self, device_id: DeviceId, name: &str) {
        event!(
            Level::INFO,
            name = name,
            device_id = device_id.to_string(),
            "Named device"
        );
        self.port_outbox.push(CoordinatorSendMessage {
            target_destinations: [device_id].into(),
            message_body: CoordinatorSendBody::Naming(frostsnap_comms::NameCommand::Finish(
                name.into(),
            )),
        })
    }

    pub fn send_cancel(&mut self, device_id: DeviceId) {
        self.port_outbox.push(CoordinatorSendMessage {
            target_destinations: [device_id].into(),
            message_body: frostsnap_comms::CoordinatorSendBody::Cancel,
        });
    }

    pub fn connected_device_labels(&self) -> BTreeMap<DeviceId, String> {
        self.registered_devices
            .clone()
            .into_iter()
            .map(|device_id| {
                (
                    device_id,
                    self.device_labels
                        .get(&device_id)
                        .expect("registered device has label")
                        .clone(),
                )
            })
            .collect()
    }

    pub fn serial_impl(&self) -> &dyn Serial {
        &*self.serial_impl
    }

    pub fn serial_impl_mut(&mut self) -> &mut dyn Serial {
        &mut *self.serial_impl
    }
}

#[derive(Debug)]
pub struct PortChanges {
    pub device_changes: Vec<DeviceChange>,
    pub new_messages: Vec<(DeviceId, DeviceToCoordinatorMessage)>,
}

#[derive(Debug, Clone)]
pub enum DeviceChange {
    Added {
        id: DeviceId,
    },
    Renamed {
        id: DeviceId,
        old_name: String,
        new_name: String,
    },
    NeedsName {
        id: DeviceId,
    },
    Registered {
        id: DeviceId,
        name: String,
    },
    Disconnected {
        id: DeviceId,
    },
}
