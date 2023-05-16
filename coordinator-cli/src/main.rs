use anyhow::{anyhow, Context};
use frostsnap_comms::DeviceReceiveSerial;
use frostsnap_comms::DeviceSendSerial;
use frostsnap_core::message::CoordinatorSend;
use frostsnap_core::message::CoordinatorToUserMessage;
use frostsnap_core::message::DeviceToCoordinatorBody;
use frostsnap_core::message::DeviceToCoordindatorMessage;
use frostsnap_core::CoordinatorFrostKey;
use frostsnap_core::DeviceId;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::str;

extern crate alloc;

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
    // Matches VID and PID
    connected: HashSet<String>,
    // Initial state
    pending: HashSet<String>,
    // After opening port and sent magic bytes
    open: HashMap<String, SerialPortBincode>,
    // Read magic magic bytes
    ready: HashMap<String, SerialPortBincode>,
    // Devices who Announce'd, mappings to port serial numbers
    device_ports: HashMap<DeviceId, String>,
    // Reverse lookup from ports to devices (daisy chaining)
    reverse_device_ports: HashMap<String, HashSet<DeviceId>>,
    // Devices we sent registration ACK to
    registered_devices: BTreeSet<DeviceId>,
}

impl Ports {
    pub fn disconnect(&mut self, port: &str) {
        self.connected.remove(port);
        self.pending.remove(port);
        self.open.remove(port);
        self.ready.remove(port);
        if let Some(device_ids) = self.reverse_device_ports.remove(port) {
            for device_id in device_ids {
                self.device_ports.remove(&device_id);
                println!("Device disconnected: {}", device_id);
            }
        }
    }

    pub fn send_to_all_devices(
        &mut self,
        send: &DeviceReceiveSerial,
    ) -> anyhow::Result<(), bincode::error::EncodeError> {
        let send_ports = self.active_ports();
        for send_port in send_ports {
            println!("Sending {} to {}", send.gist(), send_port);
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
        for serial_number in self.active_ports() {
            loop {
                let port = self.ready.get_mut(&serial_number).expect("must exist");
                match port.poll_read(None) {
                    Err(e) => {
                        eprintln!("Failed to read on port {e}");
                        self.disconnect(&serial_number);
                        break;
                    }
                    Ok(false) => break,
                    Ok(true) => {}
                }

                let decode: Result<DeviceSendSerial, _> =
                    bincode::decode_from_reader(port, bincode::config::standard());

                match decode {
                    Ok(msg) => match msg {
                        DeviceSendSerial::Core(core_message) => messages.push(core_message),
                        DeviceSendSerial::Debug { message, device } => {
                            eprintln!("Debug from device {device}: {message}")
                        }
                        DeviceSendSerial::Announce(announce_message) => {
                            eprintln!("Unexpected device announce {}", announce_message.from);
                        }
                    },
                    Err(e) => {
                        eprintln!("Unable to decode message {e}");
                        self.disconnect(&serial_number);
                    }
                }
            }
        }
        messages
    }

    fn poll_devices(&mut self) -> (BTreeSet<DeviceId>, Vec<DeviceToCoordindatorMessage>) {
        let mut device_to_coord_msg = vec![];
        let mut newly_registered = BTreeSet::new();
        let connected_now: HashSet<String> = io::find_all_ports(USB_ID).collect::<HashSet<_>>();

        let newly_connected_ports = connected_now
            .difference(&self.connected)
            .cloned()
            .collect::<Vec<_>>();
        for port in newly_connected_ports {
            println!("Port connected: {:?}", port);
            self.connected.insert(port.clone());
            self.pending.insert(port.clone());
        }

        let disconnected_ports = self
            .connected
            .difference(&connected_now)
            .cloned()
            .collect::<Vec<_>>();
        for port in disconnected_ports {
            println!("Port unplugged: {:?}", port);
            self.disconnect(&port);
        }

        for serial_number in self.pending.drain().collect::<Vec<_>>() {
            let device_port = io::open_device_port(&serial_number);
            match device_port {
                Err(e) => {
                    eprintln!("Failed to connect to device port: {:?}", e);
                }
                Ok(mut device_port) => {
                    // Write magic bytes onto JTAG
                    // println!("Trying to read magic bytes on port {}", serial_number);
                    if let Err(e) = device_port.write(&frostsnap_comms::MAGICBYTES_JTAG) {
                        eprintln!("Failed to write magic bytes: {:?}", e);
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
                    println!("Failed to read magic bytes {:?}", e);
                }
            }

            self.open.insert(serial_number, device_port);
        }

        // let mut ports_to_disconnect = HashSet::new();
        for serial_number in self.ready.keys().cloned().collect::<Vec<_>>() {
            let decoded_message: Result<DeviceSendSerial, _> = {
                let mut device_port = self.ready.get_mut(&serial_number).expect("must exist");
                let something_to_read = match device_port.poll_read(None) {
                    Err(e) => {
                        eprintln!("Failed to read on port {e}");
                        false
                    }
                    Ok(something_to_read) => something_to_read,
                };

                if something_to_read {
                    bincode::decode_from_reader(&mut device_port, bincode::config::standard())
                } else {
                    continue;
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

                        let wrote_ack = {
                            let device_port =
                                self.ready.get_mut(&serial_number).expect("must exist");

                            bincode::encode_into_writer(
                                DeviceReceiveSerial::AnnounceAck(announce.from),
                                device_port,
                                bincode::config::standard(),
                            )
                        };

                        match wrote_ack {
                            Ok(_) => {
                                println!("Registered device {}", announce.from);
                                if self.registered_devices.insert(announce.from) {
                                    newly_registered.insert(announce.from);
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "Failed to send announce Ack back to device {} {e}",
                                    announce.from
                                );
                                self.disconnect(&serial_number);
                            }
                        }

                        println!("Found device {} on {}", announce.from, serial_number);
                    }
                    DeviceSendSerial::Debug {
                        message: error,
                        device,
                    } => {
                        eprintln!("Debug: {device:?}: {error}");
                    }
                    DeviceSendSerial::Core(msg) => device_to_coord_msg.push(msg),
                },
                Err(e) => {
                    eprintln!("{:?}", e);
                }
            }
        }

        (newly_registered, device_to_coord_msg)
    }

    fn register_devices(n_devices: usize) -> Self {
        let mut ports = Ports::default();
        while ports.registered_devices.len() < n_devices {
            ports.poll_devices();
        }
        ports
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let default_db_path = home::home_dir().map(|home_dir| home_dir.join(".frostsnap"));
    let db_path = cli
        .db
        .or(default_db_path)
        .ok_or(anyhow!("We could not find home dir"))?;

    let key = if db_path.exists() {
        let key_bytes = std::fs::read(&db_path)?;
        let (key, _): (bincode::serde::Compat<CoordinatorFrostKey>, _) =
            bincode::decode_from_slice(&key_bytes, bincode::config::standard())?;
        let key = key.0;
        Some(key)
    } else {
        None
    };

    match cli.command {
        Command::Key => {
            println!("{:?}", key);
        }
        Command::Keygen {
            threshold,
            n_devices,
        } => {
            println!("Please plug in {} devices..", n_devices);

            let mut ports = Ports::register_devices(n_devices);

            if "y"
                != io::fetch_input(&format!(
                    "Want to do keygen with these devices? [y/n]\n{:?}",
                    ports.registered_devices,
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
                                                    std::fs::write(
                                                        &db_path,
                                                        bincode::encode_to_vec(
                                                            bincode::serde::Compat(key),
                                                            bincode::config::standard()).unwrap())
                                                            .context(format!("Unable to save to {}", db_path.display()))?;
                                                }
                                                finished_keygen = true;

                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error receiving message from {} {e}", message.from);
                            continue;
                        }
                    };
                }
            }
        }
        Command::Sign(sign_args) => {
            let key = key.ok_or(anyhow!("we can't sign because haven't done keygen yet!"))?;
            // LOAD FROM STATE
            let devices = key.devices();
            let threshold = key.threshold();
            let chosen_signers = choose_signers(
                &devices.into_iter().enumerate().collect::<HashMap<_, _>>(),
                threshold,
            );

            let mut still_need_to_sign = chosen_signers.clone();
            let mut coordinator = frostsnap_core::FrostCoordinator::from_stored_key(key);

            match sign_args {
                SignArgs::Message { message } => {
                    // TODO remove unwrap --> anyhow
                    let sign_request = coordinator.start_sign(message, chosen_signers).unwrap();

                    let mut ports = Ports::default();

                    println!("Plug in a signer");
                    loop {
                        let (newly_registered, new_messages) = ports.poll_devices();
                        for device in newly_registered.intersection(&still_need_to_sign) {
                            println!("Asking {} to sign", device);
                            ports.send_to_single_device(
                                &DeviceReceiveSerial::Core(sign_request.clone()),
                                device,
                            )?;
                        }

                        for message in new_messages {
                            println!("{:?}", message);
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
                                                    println!("Signature finalized:\n{}", signature);
                                                    return Ok(());
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Invalid state: {}", e)
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

fn choose_signers(devices: &HashMap<usize, DeviceId>, threshold: usize) -> BTreeSet<DeviceId> {
    println!("Choose some devices to sign:");
    for (index, device) in devices {
        println!("({}) - {}", index, device);
    }

    let mut chosen_signers: BTreeSet<DeviceId> = BTreeSet::new();
    while chosen_signers.len() < threshold {
        let choice = io::fetch_input("\nEnter a signer index (n): ").parse::<usize>();
        match choice {
            Ok(n) => match devices.get(&n) {
                Some(device_id) => {
                    if !chosen_signers.contains(device_id) {
                        chosen_signers.insert(device_id.clone());
                    } else {
                        eprintln!("Already chose this signer!")
                    }
                }
                None => todo!(),
            },
            Err(_) => {
                eprintln!("Invalid choice!")
            }
        }
    }
    chosen_signers
}
