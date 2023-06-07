use frostsnap_comms::DeviceReceiveMessage;
use frostsnap_comms::DeviceReceiveSerial;
use frostsnap_comms::DeviceSendSerial;
use frostsnap_comms::{DeviceReceiveBody, DeviceSendMessage};

use frostsnap_comms::Downstream;
use frostsnap_core::message::DeviceToCoordindatorMessage;
use frostsnap_core::DeviceId;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::{event, span, Level};

use crate::io;
use crate::serial_rw::SerialPortBincode;

// USB CDC vid and pid
const USB_ID: (u16, u16) = (12346, 4097);

#[derive(Default)]
pub struct Ports {
    /// Matches VID and PID
    connected: HashSet<String>,
    /// Initial state
    pending: HashSet<String>,
    /// After opening port and awaiting magic bytes
    awaiting_magic: HashMap<String, SerialPortBincode>,
    /// Read magic magic bytes
    ready: HashMap<String, SerialPortBincode>,
    /// ports that seems to be busy
    ignored: HashSet<String>,
    /// Devices who Announce'd, mappings to port serial numbers
    device_ports: HashMap<DeviceId, String>,
    /// Reverse lookup from ports to devices (daisy chaining)
    reverse_device_ports: HashMap<String, HashSet<DeviceId>>,
    /// Devices we sent registration ACK to
    registered_devices: BTreeSet<DeviceId>,
    /// Device labels
    device_labels: HashMap<DeviceId, String>,
    /// Messages to devices outbox
    port_outbox: Vec<DeviceReceiveMessage>,
}

impl Ports {
    pub fn disconnect(&mut self, port: &str) {
        event!(Level::INFO, port = port, "disconnecting port");
        self.connected.remove(port);
        self.pending.remove(port);
        self.awaiting_magic.remove(port);
        self.ready.remove(port);
        self.ignored.remove(port);
        if let Some(device_ids) = self.reverse_device_ports.remove(port) {
            for device_id in device_ids {
                self.device_ports.remove(&device_id);
                event!(
                    Level::DEBUG,
                    port = port,
                    device_id = device_id.to_string(),
                    "removing device because of disconnected port"
                )
            }
        }
    }

    pub fn send_to_devices(&mut self) -> anyhow::Result<()> {
        let mut leftover_sends = vec![];
        for mut send in self.port_outbox.drain(..) {
            // We have a send that has target_destinations
            // We need to determine which target destinations are connected on which ports
            // We overwrite the target destinations with any non-connected devices at the end
            let remaining_target_recipients = send.target_destinations.clone();

            let mut still_need_to_send = BTreeSet::new();
            let ports_to_send_on = remaining_target_recipients
                .into_iter()
                .filter_map(|device_id| match self.device_ports.get(&device_id) {
                    Some(serial_number) => Some(serial_number.clone()),
                    None => {
                        still_need_to_send.insert(device_id);
                        None
                    }
                })
                .collect::<BTreeSet<String>>();

            for serial_number in ports_to_send_on {
                let port = self.ready.get_mut(&serial_number).expect("must exist");

                event!(Level::DEBUG, "sending {:?}", send.message_body);
                bincode::encode_into_writer(
                    DeviceReceiveSerial::<Downstream>::Message(send.clone()),
                    port,
                    bincode::config::standard(),
                )?;
            }

            if still_need_to_send.len() > 0 {
                send.target_destinations = still_need_to_send;
                leftover_sends.push(send);
            }
        }

        self.port_outbox = leftover_sends;

        Ok(())
    }

    pub fn queue_in_port_outbox(&mut self, sends: Vec<DeviceReceiveMessage>) {
        self.port_outbox.extend(sends);
    }

    pub fn active_ports(&self) -> HashSet<String> {
        self.registered_devices
            .iter()
            .filter_map(|device_id| self.device_ports.get(device_id))
            .cloned()
            .collect::<HashSet<_>>()
    }

    pub fn receive_messages(&mut self) -> Vec<DeviceToCoordindatorMessage> {
        let mut messages = vec![];

        loop {
            let (_new_devices, mut new_messages) = self.poll_devices();
            if new_messages.is_empty() {
                break;
            }
            messages.append(&mut new_messages);
        }

        messages
    }

    pub fn poll_devices(&mut self) -> (BTreeSet<DeviceId>, Vec<DeviceToCoordindatorMessage>) {
        let span = span!(Level::DEBUG, "poll_devices");
        let _enter = span.enter();
        let mut device_to_coord_msg = vec![];
        let mut newly_registered = BTreeSet::new();
        let connected_now: HashSet<String> = io::find_all_ports(USB_ID).collect::<HashSet<_>>();

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
            self.disconnect(&port);
        }

        for serial_number in self.pending.drain().collect::<Vec<_>>() {
            let device_port = io::open_device_port(&serial_number);
            match device_port {
                Err(e) => {
                    if &e.to_string() == "Device or resource busy" {
                        if !self.ignored.contains(&serial_number) {
                            event!(
                                Level::ERROR,
                                port = serial_number,
                                "Could not open port because it's being used by another process"
                            );
                            self.ignored.insert(serial_number.clone());
                        }
                    } else {
                        event!(
                            Level::ERROR,
                            port = serial_number,
                            error = e.to_string(),
                            "Failed to open port"
                        );
                    }
                }
                Ok(mut device_port) => match device_port.write_magic_bytes() {
                    Ok(_) => {
                        self.awaiting_magic
                            .insert(serial_number.clone(), device_port);
                        // println!("Wrote magic bytes on device {}", serial_number);
                        continue;
                    }
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            port = serial_number,
                            e = e.to_string(),
                            "Failed to initialize port by writing magic bytes"
                        );
                        self.disconnect(&serial_number);
                    }
                },
            }
            self.pending.insert(serial_number);
        }

        for (serial_number, mut device_port) in self.awaiting_magic.drain().collect::<Vec<_>>() {
            match device_port.read_for_magic_bytes() {
                Ok(true) => {
                    event!(Level::DEBUG, port = serial_number, "Read magic bytes");
                    self.ready.insert(serial_number, device_port);
                    continue;
                }
                Ok(false) => {
                    // println!("Did not read magic bytes {}", serial_number);
                    match device_port.write_magic_bytes() {
                        Ok(_) => {
                            // println!("Wrote magic bytes on device {}", serial_number);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                port = serial_number,
                                e = e.to_string(),
                                "Failed to write magic bytes"
                            );
                            self.disconnect(&serial_number);
                        }
                    }
                }
                Err(e) => {
                    event!(
                        Level::DEBUG,
                        port = serial_number,
                        "failed to read magic bytes: {e}"
                    );
                }
            }
            self.awaiting_magic.insert(serial_number, device_port);
        }

        // Read all messages from ready devices
        for serial_number in self.ready.keys().cloned().collect::<Vec<_>>() {
            let decoded_message: Result<DeviceSendSerial<Downstream>, _> = {
                let mut device_port = self.ready.get_mut(&serial_number).expect("must exist");
                match device_port.poll_read(None) {
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            port = serial_number,
                            error = e.to_string(),
                            "failed to poll port for reading"
                        );
                        self.disconnect(&serial_number);
                        continue;
                    }
                    Ok(true) => {
                        event!(Level::DEBUG, port = serial_number, "ready to read");
                        bincode::decode_from_reader(&mut device_port, bincode::config::standard())
                    }
                    Ok(false) => continue,
                }
            };

            match decoded_message {
                Ok(msg) => match msg {
                    DeviceSendSerial::MagicBytes(_) => {
                        event!(Level::ERROR, port = serial_number, "Unexpected magic bytes");
                        self.disconnect(&serial_number);
                    }
                    DeviceSendSerial::Message(message) => match message {
                        DeviceSendMessage::Announce(announce) => {
                            self.device_ports
                                .insert(announce.from, serial_number.clone());
                            let devices = self
                                .reverse_device_ports
                                .entry(serial_number.clone())
                                .or_default();
                            devices.insert(announce.from);

                            event!(
                                Level::DEBUG,
                                port = serial_number,
                                id = announce.from.to_string(),
                                "Announced!"
                            );
                        }
                        DeviceSendMessage::Debug { message, device } => {
                            event!(
                                Level::DEBUG,
                                port = serial_number,
                                from = device.to_string(),
                                name = self
                                    .device_labels
                                    .get(&device)
                                    .cloned()
                                    .unwrap_or("<unknown>".into()),
                                message
                            );
                        }
                        DeviceSendMessage::Core(msg) => device_to_coord_msg.push(msg),
                    },
                },
                Err(e) => {
                    event!(
                        Level::ERROR,
                        port = serial_number,
                        error = e.to_string(),
                        "failed to read message from port"
                    );
                    self.disconnect(&serial_number);
                }
            }
        }

        for (device_id, serial_number) in self.device_ports.clone() {
            if self.registered_devices.contains(&device_id) {
                continue;
            }

            if let Some(device_label) = self.device_labels.get(&device_id) {
                let wrote_ack = {
                    let device_port = self.ready.get_mut(&serial_number).expect("must exist");

                    device_port.send_message(DeviceReceiveSerial::Message(DeviceReceiveMessage {
                        message_body: DeviceReceiveBody::AnnounceAck {
                            device_id,
                            device_label: device_label.to_string(),
                        },
                        target_destinations: BTreeSet::from([device_id]),
                    }))
                };

                match wrote_ack {
                    Ok(_) => {
                        event!(
                            Level::INFO,
                            device_id = device_id.to_string(),
                            "Registered device"
                        );
                        if self.registered_devices.insert(device_id) {
                            newly_registered.insert(device_id);
                        }
                    }
                    Err(e) => {
                        event!(
                            Level::ERROR,
                            port = serial_number,
                            error = e.to_string(),
                            "Failed to write to port to Ack announcement"
                        );
                        self.disconnect(&serial_number);
                    }
                }
            }
        }

        (newly_registered, device_to_coord_msg)
    }

    pub fn device_labels(&mut self) -> &mut HashMap<DeviceId, String> {
        &mut self.device_labels
    }

    pub fn unlabelled_devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_ports
            .keys()
            .cloned()
            .filter(|device_id| !self.device_labels.contains_key(device_id))
    }

    pub fn registered_devices(&self) -> &BTreeSet<DeviceId> {
        &self.registered_devices
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
}
