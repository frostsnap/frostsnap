use anyhow::anyhow;
use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::CoordinatorSend;
use serialport::SerialPort;
use std::error::Error;
use std::str;
use std::time::Duration;

extern crate alloc;
use alloc::collections::BTreeSet;

pub mod serial_rw;
use crate::serial_rw::SerialPortBincode;

fn open_device_port(usb_id: (u16, u16)) -> anyhow::Result<Box<dyn SerialPort>> {
    let ports = serialport::available_ports().unwrap();
    println!("Ports: {:?}", ports);
    let port = ports
        .into_iter()
        .find(|port| match &port.port_type {
            serialport::SerialPortType::UsbPort(port) => {
                port.vid == usb_id.0 && port.pid == usb_id.1
            }
            _ => false,
        })
        .ok_or(anyhow!("Failed to find device with matching usb_id"))?;
    Ok(serialport::new(&port.port_name, 9600)
        .timeout(Duration::from_millis(10))
        .open()?)
}

fn wait_for_device_port(usb_id: (u16, u16)) -> Box<dyn SerialPort> {
    loop {
        match open_device_port(usb_id) {
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
    let ports = serialport::available_ports().unwrap();
    println!("{:?}", ports);
    // ESP32-C3 USB CDC vid and pid
    // let usb_id: (u16, u16) = (4292, 60000);
    let usb_id: (u16, u16) = (12346, 4097);
    println!("Waiting for device {:?}", usb_id);
    let mut port_rw = SerialPortBincode::new(wait_for_device_port(usb_id));
    println!("Connected to device port");
    let mut coordinator = frostsnap_core::FrostCoordinator::new();
    let mut devices = BTreeSet::new();

    loop {
        println!("\n------------------------------------------------------------------");
        println!("Registered devices: {:?}", &devices);
        println!(
            "Bytes in buffer {:?} -- Bytes to read: {:?}",
            port_rw.buffer.len(),
            port_rw.port.bytes_to_read()
        );
        // std::thread::sleep(Duration::from_millis(1000));
        let choice = fetch_input(
            "\nPress:\n\tm - Read for device magic bytes\n\tr - read\n\tw - write\n\tk - start keygen\n\ts - start signing\n",
        );
        let mut sends = if choice == "w" {
            vec![DeviceReceiveSerial::AnnounceCoordinator(
                "Im a laptop".to_string(),
            )]
        } else if choice == "r" {
            if let Err(e) = port_rw.read_into_buffer() {
                eprintln!("Failed to read into buffer: {:?}", e);
            }
            for byte in &port_rw.buffer {
                print!("{:02X}", byte);
            }
            println!("");

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
            loop {
                // Write magic bytes onto JTAG
                if let Err(e) = port_rw.port.write(&frostsnap_comms::MAGICBYTES_JTAG) {
                    println!("Failed to write magic bytes: {:?}", e);
                    // drop(port_rw);
                    // port_rw = SerialPortBincode::new(wait_for_device_port(usb_id));
                    // println!("Reconnected");
                }
                std::thread::sleep(Duration::from_millis(50));

                // Read for magic bytes response
                match read_for_magic_bytes(&mut port_rw, &frostsnap_comms::MAGICBYTES_JTAG) {
                    Ok(found_magic_bytes) => {
                        if found_magic_bytes {
                            println!("Found magic bytes!!");
                            break;
                        }
                    }
                    Err(e) => {
                        println!("Failed to read magic bytes {:?}", e);
                        port_rw = SerialPortBincode::new(wait_for_device_port(usb_id));
                    }
                }
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
