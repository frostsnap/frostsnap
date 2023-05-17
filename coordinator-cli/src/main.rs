use anyhow::anyhow;
use frostsnap_comms::DeviceReceiveSerial;
use frostsnap_comms::DeviceSendSerial;
use frostsnap_core::message::CoordinatorSend;
use frostsnap_core::message::CoordinatorToUserMessage;
use frostsnap_core::message::DeviceToCoordinatorBody;
use frostsnap_core::message::DeviceToCoordindatorMessage;
use frostsnap_core::DeviceId;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::str;
use tracing::{event, span, Level};

extern crate alloc;

pub mod db;
mod device_namer;
pub mod io;
pub mod serial_rw;
use crate::serial_rw::SerialPortBincode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(short, long, value_name = "FILE")]
    db: Option<PathBuf>,
    #[arg(short)]
    verbosity: bool,
}

#[derive(Subcommand)]
enum Command {
    Keygen {
        #[arg(short, long)]
        threshold: usize,
        #[arg(short, long)]
        n_devices: usize,
    },
    Key,
    #[command(subcommand)]
    Sign(SignArgs),
}

#[derive(Subcommand)]
enum SignArgs {
    Message {
        #[arg(value_name = "message")]
        message: String,
    },
    Nostr {
        #[arg(value_name = "message")]
        message: String,
    },
    Transaction {
        #[arg(value_name = "PSBT")]
        psbt_file: PathBuf,
    },
}

// USB CDC vid and pid
const USB_ID: (u16, u16) = (12346, 4097);

#[derive(Default)]
struct Ports {
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
    ) -> anyhow::Result<(), bincode::error::EncodeError> {
        // TODO handle missing devices
        let port_serial_number = self.device_ports.get(device_id).unwrap();
        let port = self.ready.get_mut(port_serial_number).expect("must exist");

        bincode::encode_into_writer(send, port, bincode::config::standard())
    }

    fn active_ports(&self) -> HashSet<String> {
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

    fn poll_devices(&mut self) -> (BTreeSet<DeviceId>, Vec<DeviceToCoordindatorMessage>) {
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(if cli.verbosity {
            Level::DEBUG
        } else {
            Level::INFO
        })
        .pretty()
        .finish();
    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber)?;

    let default_db_path = home::home_dir().map(|home_dir| home_dir.join(".frostsnap"));
    // use that subscriber to process traces emitted after this point

    let db_path = cli
        .db
        .or(default_db_path)
        .ok_or(anyhow!("We could not find home dir"))?;

    let db = db::Db::new(db_path);
    let state = db.load()?;

    let mut ports = Ports::default();

    if let Some(state) = &state {
        *ports.device_labels() = state.device_labels.clone();
    }

    match cli.command {
        Command::Key => match state {
            Some(state) => {
                println!("{:?}", state.key);
            }
            None => eprintln!("You have not generated a key yet!"),
        },
        Command::Keygen {
            threshold,
            n_devices,
        } => {
            eprintln!("Please plug in {} devices..", n_devices);

            while ports.registered_devices.len() < n_devices {
                ports.poll_devices();

                for device_id in ports.unlabelled_devices().collect::<Vec<_>>() {
                    let device_label = device_namer::gen_name39();
                    eprintln!("Registered new device: {}", device_label);
                    ports.device_labels().insert(device_id, device_label);
                }
            }

            if "y"
                != io::fetch_input(&format!(
                    "Want to do keygen with these devices? [y/n]\n{}",
                    ports
                        .registered_devices
                        .clone()
                        .into_iter()
                        .map(|device_id| ports
                            .device_labels()
                            .get(&device_id)
                            .expect("must exist")
                            .clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ))
            {
                return Ok(());
            };

            let mut coordinator = frostsnap_core::FrostCoordinator::new();

            let do_keygen_message = DeviceReceiveSerial::Core(
                coordinator.do_keygen(&ports.registered_devices, threshold)?,
            );
            ports.send_to_all_devices(&do_keygen_message)?;

            let mut finished_keygen = false;
            while !finished_keygen {
                let new_messages = ports.receive_messages();
                for message in new_messages {
                    match coordinator.recv_device_message(message.clone()) {
                        Ok(responses) => {
                            for response in responses {
                                match response {
                                    CoordinatorSend::ToDevice(core_message) => {
                                        ports.send_to_all_devices(&DeviceReceiveSerial::Core(
                                            core_message,
                                        ))?;
                                    }
                                    CoordinatorSend::ToUser(to_user_message) => {
                                        match to_user_message {
                                            frostsnap_core::message::CoordinatorToUserMessage::Signed { .. } => {}
                                            frostsnap_core::message::CoordinatorToUserMessage::CheckKeyGen {
                                                xpub
                                            } => {
                                                let ack = io::fetch_input(&format!("OK? [y/n]: {}", xpub)) == "y";
                                                if let Some(key) = coordinator.keygen_ack(ack).unwrap() {
                                                    db.save(db::State {
                                                        key,
                                                        device_labels: ports.device_labels.clone(),
                                                    })?;
                                                }
                                                finished_keygen = true;

                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                "Failed to receive message from {}: {}",
                                message.from,
                                e
                            );
                            continue;
                        }
                    };
                }
            }
        }
        Command::Sign(sign_args) => {
            let state = state.ok_or(anyhow!("we can't sign because haven't done keygen yet!"))?;
            let key = state.key;
            let threshold = key.threshold();

            let chosen_signers = if state.device_labels.len() == threshold {
                state.device_labels.keys().cloned().collect::<BTreeSet<_>>()
            } else {
                choose_signers(&state.device_labels, threshold)
            };

            let mut still_need_to_sign = chosen_signers.clone();
            let mut coordinator = frostsnap_core::FrostCoordinator::from_stored_key(key);

            match sign_args {
                SignArgs::Message { message } => {
                    // TODO remove unwrap --> anyhow
                    let sign_request = coordinator.start_sign(message, chosen_signers).unwrap();

                    eprintln!(
                        "Plug signers:\n{}",
                        still_need_to_sign
                            .iter()
                            .map(|d| d.to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    loop {
                        let (newly_registered, new_messages) = ports.poll_devices();
                        for device in newly_registered.intersection(&still_need_to_sign) {
                            event!(Level::INFO, "asking {} to sign", device);
                            ports.send_to_single_device(
                                &DeviceReceiveSerial::Core(sign_request.clone()),
                                device,
                            )?;
                        }

                        for message in new_messages {
                            match coordinator.recv_device_message(message.clone()) {
                                Ok(responses) => {
                                    for response in responses {
                                        match response {
                                            CoordinatorSend::ToDevice(core_message) => {
                                                // TODO: Send response back to particular device?
                                                ports.send_to_all_devices(
                                                    &DeviceReceiveSerial::Core(core_message),
                                                )?;
                                            }
                                            CoordinatorSend::ToUser(user_message) => {
                                                if let CoordinatorToUserMessage::Signed {
                                                    signature,
                                                } = user_message
                                                {
                                                    event!(Level::INFO, "signing complete 🎉");
                                                    println!("{}", signature);
                                                    return Ok(());
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    event!(
                                        Level::ERROR,
                                        error = e.to_string(),
                                        from = message.from.to_string(),
                                        "got invalid message"
                                    );
                                }
                            }

                            if let DeviceToCoordindatorMessage {
                                from,
                                body: DeviceToCoordinatorBody::SignatureShare { .. },
                            } = &message
                            {
                                still_need_to_sign.remove(from);
                            }
                        }
                    }
                }
                SignArgs::Nostr { .. } => todo!(),
                SignArgs::Transaction { .. } => todo!(),
            }
        }
    }
    Ok(())
}

fn choose_signers(
    device_labels: &HashMap<DeviceId, String>,
    threshold: usize,
) -> BTreeSet<DeviceId> {
    eprintln!("Choose {} devices to sign:", threshold);
    let devices_vec = device_labels.iter().collect::<Vec<_>>();
    for (index, (_, device_label)) in devices_vec.iter().enumerate() {
        eprintln!("({}) - {}", index, device_label);
    }

    let mut chosen_signers: BTreeSet<DeviceId> = BTreeSet::new();
    while chosen_signers.len() < threshold {
        let choice = io::fetch_input("\nEnter a signer index (n): ").parse::<usize>();
        match choice {
            Ok(n) => match devices_vec.get(n) {
                Some((device_id, _)) => {
                    if !chosen_signers.contains(device_id) {
                        chosen_signers.insert(**device_id);
                    } else {
                        eprintln!("Already chose this signer!")
                    }
                }
                None => eprintln!("no such device ({}", n),
            },
            Err(_) => {
                eprintln!("Choose a number 0..{}", devices_vec.len() - 1);
            }
        }
    }
    chosen_signers
}
