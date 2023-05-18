use frostsnap_comms::DeviceReceiveSerial;
use frostsnap_comms::DeviceSendSerial;

use frostsnap_core::message::DeviceToCoordindatorMessage;
use frostsnap_core::DeviceId;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::{event, span, Level};

use crate::io;
use crate::serial_rw::SerialPortBincode;
use anyhow::anyhow;

// USB CDC vid and pid
const USB_ID: (u16, u16) = (12346, 4097);

#[derive(Default)]
pub struct Ports {
    /// Matches VID and PID
    connected: HashSet<String>,
    /// Initial state
    pending: HashSet<String>,
    /// After opening port and sent magic bytes
    open: HashMap<String, SerialPortBincode>,
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
}

impl Ports {
    pub fn disconnect(&mut self, port: &str) {
        event!(Level::INFO, port = port, "disconnecting port");
        self.connected.remove(port);
        self.pending.remove(port);
        self.open.remove(port);
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

    pub fn send_to_all_devices(
        &mut self,
        send: &DeviceReceiveSerial,
    ) -> anyhow::Result<(), bincode::error::EncodeError> {
        let send_ports = self.active_ports();
        for send_port in send_ports {
            event!(
                Level::DEBUG,
                port = send_port,
                "sending message to devices on port"
            );
            let port = self.ready.get_mut(&send_port).expect("must exist");
            bincode::encode_into_writer(send, port, bincode::config::standard())?
        }
        Ok(())
    }

    pub fn send_to_single_device(
        &mut self,
        send: &DeviceReceiveSerial,
        device_id: &DeviceId,
    ) -> anyhow::Result<()> {
        let port_serial_number = self
            .device_ports
            .get(device_id)
            .ok_or(anyhow!("Device not connected!"))?;
        let port = self.ready.get_mut(port_serial_number).expect("must exist");

        Ok(bincode::encode_into_writer(
            send,
            port,
            bincode::config::standard(),
        )?)
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
            event!(Level::DEBUG, port = port.to_string(), "USB port connected");
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
                Ok(mut device_port) => {
                    // Write magic bytes onto JTAG
                    // println!("Trying to read magic bytes on port {}", serial_number);
                    if let Err(e) = device_port.write(&frostsnap_comms::MAGICBYTES_JTAG) {
                        event!(
                            Level::ERROR,
                            port = serial_number,
                            e = e.to_string(),
                            "Failed to initialize port by writing magic bytes"
                        );
                        self.disconnect(&serial_number);
                    } else {
                        self.open.insert(
                            serial_number.clone(),
                            SerialPortBincode::new(device_port, serial_number),
                        );
                        continue;
                    }
                }
            }
            self.pending.insert(serial_number);
        }

        for (serial_number, mut device_port) in self.open.drain().collect::<Vec<_>>() {
            match io::read_for_magic_bytes(&mut device_port, &frostsnap_comms::MAGICBYTES_JTAG) {
                Ok(true) => {
                    // println!("Found magic bytes on device {}", serial_number);
                    self.ready.insert(serial_number, device_port);
                    continue;
                }
                Ok(false) => { /* magic bytes haven't been read yet */ }
                Err(e) => {
                    event!(
                        Level::ERROR,
                        port = serial_number,
                        e = e.to_string(),
                        "Failed to initialize port by reading magic bytes"
                    );
                    self.disconnect(&serial_number);
                }
            }

            self.open.insert(serial_number, device_port);
        }

        // Read all messages from ready devices
        for serial_number in self.ready.keys().cloned().collect::<Vec<_>>() {
            let decoded_message: Result<DeviceSendSerial, _> = {
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
                        bincode::decode_from_reader(&mut device_port, bincode::config::standard())
                    }
                    Ok(false) => continue,
                }
            };

            match decoded_message {
                Ok(msg) => match msg {
                    DeviceSendSerial::Announce(announce) => {
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
                    DeviceSendSerial::Debug { message, device } => {
                        event!(
                            Level::DEBUG,
                            port = serial_number,
                            from = device.to_string(),
                            message
                        );
                    }
                    DeviceSendSerial::Core(msg) => device_to_coord_msg.push(msg),
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

                    bincode::encode_into_writer(
                        DeviceReceiveSerial::AnnounceAck {
                            device_id,
                            device_label: device_label.to_string(),
                        },
                        device_port,
                        bincode::config::standard(),
                    )
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
