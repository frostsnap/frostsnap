use anyhow::anyhow;
use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::{CoordinatorSend, CoordinatorToDeviceMessage};
use serialport::SerialPort;
use std::collections::BTreeMap;
use std::error::Error;
use std::str;
use std::time::Duration;

extern crate alloc;
use alloc::collections::BTreeSet;

pub mod serial_rw;
use crate::serial_rw::SerialPortBincode;

fn find_all_ports(usb_id: (u16, u16)) -> Vec<String> {
    let available_ports = serialport::available_ports().unwrap();
    let ports = available_ports
        .into_iter()
        .filter_map(|port| match &port.port_type {
            serialport::SerialPortType::UsbPort(port) => {
                if port.vid == usb_id.0 && port.pid == usb_id.1 {
                    port.serial_number.clone()
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    ports
    // port.vid == usb_id.0 && port.pid == usb_id.1
}

fn open_device_port(serial_number: &str) -> anyhow::Result<Box<dyn SerialPort>> {
    let available_ports = serialport::available_ports().unwrap();
    println!("Ports: {:?}", available_ports);
    let port = available_ports
        .into_iter()
        .find(|port| match &port.port_type {
            serialport::SerialPortType::UsbPort(port) => {
                if let Some(port_serial_number) = port.serial_number.clone() {
                    port_serial_number == serial_number
                } else {
                    false
                }
            }
            _ => false,
        })
        .ok_or(anyhow!("Failed to find device with matching usb_id"))?;
    Ok(serialport::new(&port.port_name, 9600)
        .timeout(Duration::from_millis(10))
        .open()?)
}

fn wait_for_device_port(serial_number: &str) -> Box<dyn SerialPort> {
    loop {
        match open_device_port(serial_number) {
            Ok(port) => return port,
            Err(e) => eprintln!("Error opening port {:?}", e),
        }
        std::thread::sleep(std::time::Duration::from_secs(1))
    }
}

pub fn read_for_magic_bytes(
    port_rw: &mut SerialPortBincode,
    magic_bytes: &[u8],
) -> Result<bool, std::io::Error> {
    let n = port_rw.port.bytes_to_read()? as usize;
    let mut buffer = vec![0u8; n];

    match port_rw.port.read(&mut buffer) {
        Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
        Ok(_) => port_rw.buffer.append(&mut buffer),
        Err(e) => return Err(e),
    };
    let position = port_rw
        .buffer
        .windows(magic_bytes.len())
        .position(|window| window == &magic_bytes[..]);
    match position {
        Some(position) => {
            println!("Read magic bytes");
            port_rw.buffer = port_rw.buffer.split_off(position + magic_bytes.len());
            Ok(true)
        }
        None => Ok(false),
    }
}

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
    let number_of_device_ports = 2;

    println!(
        "Waiting for {} devices to be plugged in..",
        number_of_device_ports
    );
    let connected_devices = loop {
        let connected_devices = find_all_ports(usb_id);
        if connected_devices.len() == number_of_device_ports {
            break connected_devices;
        }
    };

    println!(
        "Trying to connect to device ports: {:?}",
        &connected_devices
    );

    let mut ports: Vec<_> = connected_devices
        .into_iter()
        .map(|serial_number| {
            SerialPortBincode::new(wait_for_device_port(&serial_number), serial_number)
        })
        .collect();
    println!("Connected to devices.");

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
            "\nPress:\n\tm - Read for device magic bytes\n\tr - read\n\tw - write\n\tk - start keygen\n\ts - start signing\n",
        );
        let sends = if choice == "w" {
            let sends = (0..ports.len())
                .map(|i| {
                    (
                        i,
                        vec![DeviceReceiveSerial::AnnounceCoordinator(
                            "Im a laptop".to_string(),
                        )],
                    )
                })
                .collect::<BTreeMap<_, _>>();
            sends
        } else if choice == "r" {
            let mut send_all_ports = BTreeMap::new();
            let mut sends = BTreeMap::new();
            for (i, mut port_rw) in ports.iter_mut().enumerate() {
                println!("Reading port {}", i);
                if let Err(e) = port_rw.read_into_buffer() {
                    eprintln!("Failed to read port {} into buffer: {:?}", i, e);
                }
                for byte in &port_rw.buffer {
                    print!("{:02X}", byte);
                }
                println!("");

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

                for new_send in new_sends.clone() {
                    if let DeviceReceiveSerial::Core(core_message) = &new_send {
                        if let CoordinatorToDeviceMessage::FinishKeyGen { .. } = core_message {
                            send_all_ports.insert(i, new_send);
                        }
                    };
                }
                sends.insert(i, new_sends);
            }

            // Some messages need to be sent to ALL ports
            for (port_index, send_messages) in sends.iter_mut() {
                for (original_dest, send_all_message) in &send_all_ports {
                    if original_dest != port_index {
                        send_messages.push(send_all_message.clone());
                    }
                }
            }

            sends
        } else if choice == "k" {
            let do_keygen_message: Vec<_> = coordinator
                .do_keygen(&devices, devices.len())
                .unwrap()
                .into_iter()
                .map(|msg| DeviceReceiveSerial::Core(msg))
                .collect();
            let sends = (0..ports.len())
                .map(|i| (i, do_keygen_message.clone()))
                .collect::<BTreeMap<_, _>>();
            sends
        } else if choice == "s" {
            let message_to_sign = fetch_input("Enter a message to be signed: ");
            let sign_messages: Vec<_> = coordinator
                .start_sign(message_to_sign, devices.clone())
                .unwrap()
                .into_iter()
                .map(|msg| DeviceReceiveSerial::Core(msg))
                .collect();
            let sends = (0..ports.len())
                .map(|i| (i, sign_messages.clone()))
                .collect::<BTreeMap<_, _>>();
            sends
        } else if choice == "m" {
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
                    std::thread::sleep(Duration::from_millis(50));

                    // Read for magic bytes response
                    match read_for_magic_bytes(port_rw, &frostsnap_comms::MAGICBYTES_JTAG) {
                        Ok(found_magic_bytes) => {
                            if found_magic_bytes {
                                println!("Found magic bytes!!");
                                break;
                            }
                        }
                        Err(e) => {
                            println!("Failed to read magic bytes {:?}", e);
                            *port_rw = SerialPortBincode::new(
                                wait_for_device_port(&port_rw.serial_number),
                                port_rw.serial_number.clone(),
                            );
                        }
                    }
                }
            }

            BTreeMap::new()
        } else {
            println!("Did nothing..");
            BTreeMap::new()
        };

        println!("Sending these messages:");
        for (port_index, port_sends) in sends {
            // for send in port_sends.drain(..) {
            for send in port_sends {
                println!("{:?}", send);
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
                    }
                }

                println!("");
            }
        }
    }
}
