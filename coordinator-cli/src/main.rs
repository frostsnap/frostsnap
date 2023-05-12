use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::{CoordinatorSend, CoordinatorToDeviceMessage};
use std::error::Error;
use std::str;
use std::time::Duration;

extern crate alloc;
use alloc::collections::BTreeSet;

pub mod io;
pub mod serial_rw;
use crate::serial_rw::SerialPortBincode;

fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("can not read user input");
    let cleaned_input = input.trim().to_string();
    cleaned_input
}

fn fetch_input(prompt: &str) -> String {
    println!("{}", prompt);
    read_string()
}

fn main() -> Result<(), Box<dyn Error>> {
    // ESP32-C3 USB CDC vid and pid
    // let usb_id: (u16, u16) = (4292, 60000);
    let usb_id: (u16, u16) = (12346, 4097);
    let connected_devices = loop {
        println!("\n\n");
        let connected_devices = io::find_all_ports(usb_id);
        for device in &connected_devices {
            println!("{:?}", device);
        }
        if fetch_input("All devices connected? (y)") == "y" {
            break connected_devices;
        };
    };

    println!(
        "Trying to connect to device ports: {:?}",
        &connected_devices
    );

    let mut ports: Vec<_> = connected_devices
        .into_iter()
        .map(|serial_number| {
            SerialPortBincode::new(io::wait_for_device_port(&serial_number), serial_number)
        })
        .collect();
    println!("Connected to devices. Reading for magic bytes...");

    // Read magic bytes on each port
    for (i, port_rw) in ports.iter_mut().enumerate() {
        loop {
            // Write magic bytes onto JTAG
            println!("Trying to read magic bytes on port {}", i);
            if let Err(e) = port_rw.port.write(&frostsnap_comms::MAGICBYTES_JTAG) {
                println!("Failed to write magic bytes: {:?}", e);
                // drop(port_rw);
                // *port_rw = SerialPortBincode::new(
                //     wait_for_device_port(&port_rw.serial_number),
                //     port_rw.serial_number.clone(),
                // );
                // println!("Reconnected");
            }
            std::thread::sleep(Duration::from_millis(500));

            // Read for magic bytes response
            match io::read_for_magic_bytes(port_rw, &frostsnap_comms::MAGICBYTES_JTAG) {
                Ok(found_magic_bytes) => {
                    if found_magic_bytes {
                        println!("Found magic bytes!!");
                        break;
                    }
                }
                Err(e) => {
                    println!("Failed to read magic bytes {:?}", e);
                    *port_rw = SerialPortBincode::new(
                        io::wait_for_device_port(&port_rw.serial_number),
                        port_rw.serial_number.clone(),
                    );
                }
            }
        }
    }

    let mut coordinator = frostsnap_core::FrostCoordinator::new();
    let mut devices = BTreeSet::new();

    loop {
        println!("\n------------------------------------------------------------------");
        println!("Registered devices: {:?}", &devices);
        for (i, port_rw) in ports.iter().enumerate() {
            println!(
                "Port {} bytes in buffer {:?} -- Bytes to read: {:?}",
                i,
                port_rw.buffer.len(),
                port_rw.port.bytes_to_read()
            );
        }
        // std::thread::sleep(Duration::from_millis(1000));
        let choice = fetch_input(
            "\nPress:\n\tr - read messages\n\tw - announce self\n\tk - start keygen\n\ts - start signing\n",
        );
        let sends = if choice == "w" {
            let sends = (0..ports.len())
                .map(|i| {
                    (
                        i,
                        DeviceReceiveSerial::AnnounceCoordinator("Im a laptop".to_string()),
                    )
                })
                .collect::<Vec<_>>();
            sends
        } else if choice == "r" {
            let mut send_all_ports = vec![];
            let mut sends = vec![];
            let n_ports = ports.len();
            for (port_index, mut port_rw) in ports.iter_mut().enumerate() {
                println!("Reading port {}", port_index);
                if let Err(e) = port_rw.read_into_buffer() {
                    eprintln!("Failed to read port {} into buffer: {:?}", port_index, e);
                }
                // for byte in &port_rw.buffer {
                //     print!("{:02X}", byte);
                // }
                // println!("");

                let decode: Result<DeviceSendSerial, _> =
                    bincode::decode_from_reader(&mut port_rw, bincode::config::standard());
                let new_sends = match decode {
                    Ok(msg) => {
                        match &msg {
                            DeviceSendSerial::Announce(announcement) => {
                                println!("Registered device: {:?}", announcement.from);
                                devices.insert(announcement.from);
                                vec![DeviceReceiveSerial::AnnounceAck(announcement.from)]
                            }
                            DeviceSendSerial::Core(core_msg) => {
                                println!("Read core message: {:?}", msg);

                                let our_responses =
                                    coordinator.recv_device_message(core_msg.clone()).unwrap();

                                our_responses
                                .into_iter()
                                .filter_map(|msg| match msg {
                                    CoordinatorSend::ToDevice(core_message) => {
                                        Some(DeviceReceiveSerial::Core(core_message))
                                    }
                                    CoordinatorSend::ToUser(to_user_message) => {
                                        fetch_input(&format!("Ack this message for coordinator?: {:?}", to_user_message));
                                        match to_user_message {
                                            frostsnap_core::message::CoordinatorToUserMessage::Signed { .. } => {}
                                            frostsnap_core::message::CoordinatorToUserMessage::CheckKeyGen {
                                                ..
                                            } => {
                                                coordinator.keygen_ack(true).unwrap();
                                            }
                                        }
                                        None
                                    },
                                })
                                .collect() // TODO remove panic
                            }
                            DeviceSendSerial::Debug { error, device } => {
                                println!("Debug message from {:?}: {:?}", device, error);
                                vec![]
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{:?}", e);
                        // Write something to serial to prevent device hanging
                        vec![]
                    }
                };

                // Some messages need to be shared to everyone!
                for new_send in new_sends.clone() {
                    if let DeviceReceiveSerial::Core(_) = new_send {
                        match &new_send {
                            DeviceReceiveSerial::Core(msg) => match &msg {
                                CoordinatorToDeviceMessage::DoKeyGen { .. } => {
                                    send_all_ports.push(new_send.clone())
                                }
                                CoordinatorToDeviceMessage::FinishKeyGen { .. } => {
                                    send_all_ports.push(new_send.clone())
                                }
                                CoordinatorToDeviceMessage::RequestSign { .. } => {
                                    send_all_ports.push(new_send.clone())
                                }
                            },
                            DeviceReceiveSerial::AnnounceAck(_) => {}
                            DeviceReceiveSerial::AnnounceCoordinator(_) => {
                                send_all_ports.push(new_send.clone())
                            }
                        }
                    };
                }
                // dbg!(&new_sends);
                // dbg!(&send_all_ports);
                for send_all_message in send_all_ports.iter() {
                    for other_port_index in 0..n_ports {
                        if port_index != other_port_index {
                            sends.push((other_port_index, send_all_message.clone()));
                        }
                    }
                }

                // Store sends
                for new_send in new_sends {
                    sends.push((port_index, new_send.clone()));
                }
            }

            sends
        } else if choice == "k" {
            let threshold = if devices.len() > 2 {
                devices.len() - 1
            } else {
                devices.len()
            };
            let do_keygen_message: Vec<_> = coordinator
                .do_keygen(&devices, threshold)
                .unwrap()
                .into_iter()
                .map(|msg| DeviceReceiveSerial::Core(msg))
                .collect();

            let mut sends = vec![];
            for recipient_port in 0..ports.len() {
                for msg in do_keygen_message.clone() {
                    sends.push((recipient_port, msg));
                }
            }
            sends
        } else if choice == "s" {
            let threshold = if devices.len() > 2 {
                devices.len() - 1
            } else {
                devices.len()
            };
            let message_to_sign = fetch_input("Enter a message to be signed: ");
            let sign_messages: Vec<_> = coordinator
                .start_sign(
                    message_to_sign,
                    devices.clone().into_iter().take(threshold).collect(),
                )
                .unwrap()
                .into_iter()
                .map(|msg| DeviceReceiveSerial::Core(msg))
                .collect();
            let mut sends = vec![];
            for message in sign_messages {
                for port_index in 0..ports.len() {
                    sends.push((port_index, message.clone()));
                }
            }
            sends
        } else {
            println!("Did nothing..");
            vec![]
        };

        println!("Sending these messages:");
        for (port_index, send) in sends {
            dbg!(&send);
            for (destination_port, other_port) in ports.iter_mut().enumerate() {
                if destination_port != port_index {
                    continue;
                } else {
                    if let Err(e) = bincode::encode_into_writer(
                        send.clone(),
                        other_port,
                        bincode::config::standard(),
                    ) {
                        eprintln!("Error writing message to serial {:?}", e);
                    }
                    println!("send on port {}", destination_port);
                }

                println!("");
            }
        }
    }
}
