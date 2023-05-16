use frostsnap_comms::DeviceReceiveSerial;
use frostsnap_comms::DeviceSendSerial;
use frostsnap_core::message::CoordinatorSend;
use frostsnap_core::message::DeviceToCoordindatorMessage;
use frostsnap_core::DeviceId;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
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
}

#[derive(Subcommand)]
enum Command {
    Keygen {
        #[arg(short, long)]
        threshold: usize,
        #[arg(short, long)]
        n_devices: usize,
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

    fn register_devices(n_devices: usize) -> Self {
        let mut ports = Ports::default();
        loop {
            let connected_now: HashSet<String> = io::find_all_ports(USB_ID).collect::<HashSet<_>>();

            let newly_connected_ports = connected_now
                .difference(&ports.connected)
                .cloned()
                .collect::<Vec<_>>();
            for port in newly_connected_ports {
                println!("Port connected: {:?}", port);
                ports.connected.insert(port.clone());
                ports.pending.insert(port.clone());
            }

            let disconnected_ports = ports
                .connected
                .difference(&connected_now)
                .cloned()
                .collect::<Vec<_>>();
            for port in disconnected_ports {
                println!("Port unplugged: {:?}", port);
                ports.disconnect(&port);
            }

            for serial_number in ports.pending.drain().collect::<Vec<_>>() {
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
                            ports.open.insert(
                                serial_number.clone(),
                                SerialPortBincode::new(device_port, serial_number),
                            );
                            continue;
                        }
                    }
                }
                ports.pending.insert(serial_number);
            }

            for (serial_number, mut device_port) in ports.open.drain().collect::<Vec<_>>() {
                match io::read_for_magic_bytes(&mut device_port, &frostsnap_comms::MAGICBYTES_JTAG)
                {
                    Ok(true) => {
                        // println!("Found magic bytes on device {}", serial_number);
                        ports.ready.insert(serial_number, device_port);
                        continue;
                    }
                    Ok(false) => { /* magic bytes haven't been read yet */ }
                    Err(e) => {
                        println!("Failed to read magic bytes {:?}", e);
                    }
                }

                ports.open.insert(serial_number, device_port);
            }

            // let mut ports_to_disconnect = HashSet::new();
            for serial_number in ports.ready.keys().cloned().collect::<Vec<_>>() {
                let decoded_message: Result<DeviceSendSerial, _> = {
                    let mut device_port = ports.ready.get_mut(&serial_number).expect("must exist");
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
                            ports
                                .device_ports
                                .insert(announce.from, serial_number.clone());
                            let devices = ports
                                .reverse_device_ports
                                .entry(serial_number.clone())
                                .or_default();
                            devices.insert(announce.from);

                            let wrote_ack = {
                                let device_port =
                                    ports.ready.get_mut(&serial_number).expect("must exist");

                                bincode::encode_into_writer(
                                    DeviceReceiveSerial::AnnounceAck(announce.from),
                                    device_port,
                                    bincode::config::standard(),
                                )
                            };

                            match wrote_ack {
                                Ok(_) => {
                                    println!("Registered device {}", announce.from);
                                    ports.registered_devices.insert(announce.from);
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Failed to send announce Ack back to device {} {e}",
                                        announce.from
                                    );
                                    ports.disconnect(&serial_number);
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
                        DeviceSendSerial::Core(_) => {}
                    },
                    Err(e) => {
                        eprintln!("{:?}", e);
                    }
                }
            }

            // TODO: Other conditional (should be option) ||
            if ports.device_ports.len() >= n_devices {
                break;
            }
        }
        ports
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
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
                                        io::fetch_input(&format!("OK?: {:?}", to_user_message));
                                        match to_user_message {
                                        frostsnap_core::message::CoordinatorToUserMessage::Signed { .. } => {}
                                        frostsnap_core::message::CoordinatorToUserMessage::CheckKeyGen {
                                            ..
                                        } => {
                                            coordinator.keygen_ack(true).unwrap();
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
    }
    Ok(())
}

//     println!(
//         "Trying to connect to device ports: {:?}",
//         &connected_devices
//     );

//     let mut ports: Vec<_> = connected_devices
//         .into_iter()
//         .map(|serial_number| {
//             SerialPortBincode::new(io::wait_for_device_port(&serial_number), serial_number)
//         })
//         .collect();
//     println!("Connected to devices. Reading for magic bytes...");

//     // Read magic bytes on each port
//     for (i, port_rw) in ports.iter_mut().enumerate() {
//         loop {
//             // Write magic bytes onto JTAG
//             println!("Trying to read magic bytes on port {}", i);
//             if let Err(e) = port_rw.port.write(&frostsnap_comms::MAGICBYTES_JTAG) {
//                 println!("Failed to write magic bytes: {:?}", e);
//                 // drop(port_rw);
//                 // *port_rw = SerialPortBincode::new(
//                 //     wait_for_device_port(&port_rw.serial_number),
//                 //     port_rw.serial_number.clone(),
//                 // );
//                 // println!("Reconnected");
//             }
//             std::thread::sleep(Duration::from_millis(500));

//             // Read for magic bytes response
//             match io::read_for_magic_bytes(port_rw, &frostsnap_comms::MAGICBYTES_JTAG) {
//                 Ok(found_magic_bytes) => {
//                     if found_magic_bytes {
//                         println!("Found magic bytes!!");
//                         break;
//                     }
//                 }
//                 Err(e) => {
//                     println!("Failed to read magic bytes {:?}", e);
//                     *port_rw = SerialPortBincode::new(
//                         io::wait_for_device_port(&port_rw.serial_number),
//                         port_rw.serial_number.clone(),
//                     );
//                 }
//             }
//         }
//     }

//     let mut coordinator = frostsnap_core::FrostCoordinator::new();
//     let mut devices = BTreeSet::new();

//     loop {
//         println!("\n------------------------------------------------------------------");
//         println!("Registered devices: {:?}", &devices);
//         for (i, port_rw) in ports.iter().enumerate() {
//             println!(
//                 "Port {} bytes in buffer {:?} -- Bytes to read: {:?}",
//                 i,
//                 port_rw.buffer.len(),
//                 port_rw.port.bytes_to_read()
//             );
//         }
//         // std::thread::sleep(Duration::from_millis(1000));
//         let choice = fetch_input(
//             "\nPress:\n\tr - read messages\n\tw - announce self\n\tk - start keygen\n\ts - start signing\n",
//         );
//         let sends = if choice == "w" {
//             let sends = (0..ports.len())
//                 .map(|i| {
//                     (
//                         i,
//                         DeviceReceiveSerial::AnnounceCoordinator("Im a laptop".to_string()),
//                     )
//                 })
//                 .collect::<Vec<_>>();
//             sends
//         } else if choice == "r" {
//             let mut send_all_ports = vec![];
//             let mut sends = vec![];
//             let n_ports = ports.len();
//             for (port_index, mut port_rw) in ports.iter_mut().enumerate() {
//                 println!("Reading port {}", port_index);
//                 if let Err(e) = port_rw.read_into_buffer() {
//                     eprintln!("Failed to read port {} into buffer: {:?}", port_index, e);
//                 }
//                 // for byte in &port_rw.buffer {
//                 //     print!("{:02X}", byte);
//                 // }
//                 // println!("");

//                 let decode: Result<DeviceSendSerial, _> =
//                     bincode::decode_from_reader(&mut port_rw, bincode::config::standard());
//                 let new_sends = match decode {
//                     Ok(msg) => {
//                         match &msg {
//                             DeviceSendSerial::Announce(announcement) => {
//                                 println!("Registered device: {:?}", announcement.from);
//                                 devices.insert(announcement.from);
//                                 vec![DeviceReceiveSerial::AnnounceAck(announcement.from)]
//                             }
//                             DeviceSendSerial::Core(core_msg) => {
//                                 println!("Read core message: {:?}", msg);

//                                 let our_responses =
//                                     coordinator.recv_device_message(core_msg.clone()).unwrap();

//                                 our_responses
//                                 .into_iter()
//                                 .filter_map(|msg| match msg {
//                                     CoordinatorSend::ToDevice(core_message) => {
//                                         Some(DeviceReceiveSerial::Core(core_message))
//                                     }
//                                     CoordinatorSend::ToUser(to_user_message) => {
//                                         fetch_input(&format!("Ack this message for coordinator?: {:?}", to_user_message));
//                                         match to_user_message {
//                                             frostsnap_core::message::CoordinatorToUserMessage::Signed { .. } => {}
//                                             frostsnap_core::message::CoordinatorToUserMessage::CheckKeyGen {
//                                                 ..
//                                             } => {
//                                                 coordinator.keygen_ack(true).unwrap();
//                                             }
//                                         }
//                                         None
//                                     },
//                                 })
//                                 .collect() // TODO remove panic
//                             }
//                             DeviceSendSerial::Debug { error, device } => {
//                                 println!("Debug message from {:?}: {:?}", device, error);
//                                 vec![]
//                             }
//                         }
//                     }
//                     Err(e) => {
//                         eprintln!("{:?}", e);
//                         // Write something to serial to prevent device hanging
//                         vec![]
//                     }
//                 };

//                 // Some messages need to be shared to everyone!
//                 for new_send in new_sends.clone() {
//                     if let DeviceReceiveSerial::Core(_) = new_send {
//                         match &new_send {
//                             DeviceReceiveSerial::Core(msg) => match &msg {
//                                 CoordinatorToDeviceMessage::DoKeyGen { .. } => {
//                                     send_all_ports.push(new_send.clone())
//                                 }
//                                 CoordinatorToDeviceMessage::FinishKeyGen { .. } => {
//                                     send_all_ports.push(new_send.clone())
//                                 }
//                                 CoordinatorToDeviceMessage::RequestSign { .. } => {
//                                     send_all_ports.push(new_send.clone())
//                                 }
//                             },
//                             DeviceReceiveSerial::AnnounceAck(_) => {}
//                             DeviceReceiveSerial::AnnounceCoordinator(_) => {
//                                 send_all_ports.push(new_send.clone())
//                             }
//                         }
//                     };
//                 }
//                 // dbg!(&new_sends);
//                 // dbg!(&send_all_ports);
//                 for send_all_message in send_all_ports.iter() {
//                     for other_port_index in 0..n_ports {
//                         if port_index != other_port_index {
//                             sends.push((other_port_index, send_all_message.clone()));
//                         }
//                     }
//                 }

//                 // Store sends
//                 for new_send in new_sends {
//                     sends.push((port_index, new_send.clone()));
//                 }
//             }

//             sends
//         } else if choice == "k" {
//             let threshold = if devices.len() > 2 {
//                 devices.len() - 1
//             } else {
//                 devices.len()
//             };
//             let do_keygen_message: Vec<_> = coordinator
//                 .do_keygen(&devices, threshold)
//                 .unwrap()
//                 .into_iter()
//                 .map(|msg| DeviceReceiveSerial::Core(msg))
//                 .collect();

//             let mut sends = vec![];
//             for recipient_port in 0..ports.len() {
//                 for msg in do_keygen_message.clone() {
//                     sends.push((recipient_port, msg));
//                 }
//             }
//             sends
//         } else if choice == "s" {
//             let threshold = if devices.len() > 2 {
//                 devices.len() - 1
//             } else {
//                 devices.len()
//             };
//             let message_to_sign = fetch_input("Enter a message to be signed: ");
//             let sign_messages: Vec<_> = coordinator
//                 .start_sign(
//                     message_to_sign,
//                     devices.clone().into_iter().take(threshold).collect(),
//                 )
//                 .unwrap()
//                 .into_iter()
//                 .map(|msg| DeviceReceiveSerial::Core(msg))
//                 .collect();
//             let mut sends = vec![];
//             for message in sign_messages {
//                 for port_index in 0..ports.len() {
//                     sends.push((port_index, message.clone()));
//                 }
//             }
//             sends
//         } else {
//             println!("Did nothing..");
//             vec![]
//         };

//         println!("Sending these messages:");
//         for (port_index, send) in sends {
//             dbg!(&send);
//             for (destination_port, other_port) in ports.iter_mut().enumerate() {
//                 if destination_port != port_index {
//                     continue;
//                 } else {
//                     if let Err(e) = bincode::encode_into_writer(
//                         send.clone(),
//                         other_port,
//                         bincode::config::standard(),
//                     ) {
//                         eprintln!("Error writing message to serial {:?}", e);
//                     }
//                     println!("send on port {}", destination_port);
//                 }

//                 println!("");
//             }
//         }
//     }
// }
