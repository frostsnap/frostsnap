use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::CoordinatorSend;
use std::error::Error;
use std::str;
use std::time::Duration;

extern crate alloc;
use alloc::collections::BTreeSet;

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
    println!("Finding serial devices...");
    println!("");
    let found_port: String = loop {
        let ports = serialport::available_ports()?;
        for (port_index, port) in ports.iter().enumerate() {
            println!("{:?} -- {:?}", port_index, port);
        }

        match fetch_input("Type index or enter to refresh: ").parse::<usize>() {
            Ok(index_selection) => break ports[index_selection].port_name.clone(),
            Err(_) => {}
        }
    };

    println!("Connecting to {}", found_port);
    let port = serialport::new(&found_port, 9600)
        .timeout(Duration::from_millis(10))
        .open()
        .unwrap_or_else(|e| {
            eprintln!("Failed to open \"{}\". Error: {}", &found_port, e);
            std::process::exit(1);
        });
    let mut port_rw = SerialPortBincode::new(port);

    let mut coordinator = frostsnap_core::FrostCoordinator::new();
    let mut devices = BTreeSet::new();
    loop {
        println!("\n------------------------------------------------------------------");
        println!("Registered devices: {:?}", &devices);
        // std::thread::sleep(Duration::from_millis(1000));
        let choice = fetch_input(
            "\nPress:\n\tm - Read for device magic bytes\n\tr - read\n\tw - write\n\tk - start keygen\n\ts - start signing\n",
        );
        let mut sends = if choice == "w" {
            vec![DeviceReceiveSerial::AnnounceCoordinator(
                "Im a laptop".to_string(),
            )]
        } else if choice == "r" {
            let decode: Result<DeviceSendSerial, _> =
                bincode::decode_from_reader(&mut port_rw, bincode::config::standard());
            let sends = match decode {
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
            sends
        } else if choice == "k" {
            coordinator
                .do_keygen(&devices, devices.len())
                .unwrap()
                .into_iter()
                .map(|msg| DeviceReceiveSerial::Core(msg))
                .collect()
        } else if choice == "s" {
            let message_to_sign = fetch_input("Enter a message to be signed: ");
            coordinator
                .start_sign(message_to_sign, devices.clone())
                .unwrap()
                .into_iter()
                .map(|msg| DeviceReceiveSerial::Core(msg))
                .collect()
        } else if choice == "m" {
            if port_rw.read_for_magic_bytes(10_000) {
                println!("Found magic bytes!");
            } else {
                println!("Failed to find magic bytes..");
            }
            vec![]
        } else {
            println!("Did nothing..");
            vec![]
        };

        println!("Sending these messages:");
        for send in sends.drain(..) {
            println!("{:?}", send);
            if let Err(e) =
                bincode::encode_into_writer(send, &mut port_rw, bincode::config::standard())
            {
                eprintln!("Error writing message to serial {:?}", e);
            }
            println!("");
        }
    }
}
