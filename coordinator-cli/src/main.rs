use bincode::{BorrowDecode, Decode, Encode};
use frostsnap_core::message::{
    CoordinatorToDeviceMessage, CoordinatorToDeviceSend, DeviceToCoordindatorMessage,
};
// use serde::{Deserialize, Serialize};
use std::error::Error;
use std::str;
use std::time::Duration;

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

#[derive(Encode, Debug, Clone)]
struct CoordinatorSendSerial {
    #[bincode(with_serde)]
    message: CoordinatorToDeviceSend,
}

#[derive(Decode, Debug, Clone)]
struct CoordinatorReceiveSerial {
    #[bincode(with_serde)]
    message: DeviceToCoordindatorMessage,
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
    let port = serialport::new(&found_port, 115_200)
        .timeout(Duration::from_millis(10))
        .open()
        .unwrap_or_else(|e| {
            eprintln!("Failed to open \"{}\". Error: {}", &found_port, e);
            std::process::exit(1);
        });
    let mut port_rw = SerialPortBincode::new(port);

    let mut coordinator = frostsnap_core::FrostCoordinator::new();

    loop {
        // std::thread::sleep(Duration::from_millis(1000));
        let choice = fetch_input("\nPress:\n\tr - read\n\tw - write\n");
        if choice == "w" {
            // if let Err(e) = bincode::encode_into_writer(
            //     write_message.clone(),
            //     &mut port_rw,
            //     bincode::config::standard(),
            // ) {
            //     eprintln!("{:?}", e);
            // }
            println!("Wrote nothing..");
        } else if choice == "r" {
            let decode: Result<CoordinatorReceiveSerial, _> =
                bincode::decode_from_reader(&mut port_rw, bincode::config::standard());
            let sends = match decode {
                Ok(msg) => {
                    println!("Read: {:?}", msg);
                    coordinator.recv_device_message(msg.message).unwrap()
                }
                Err(e) => {
                    eprintln!("{:?}", e);
                    vec![]
                }
            };
            for send in sends {
                match send {
                    frostsnap_core::message::CoordinatorSend::ToDevice(msg) => {
                        let serial_msg = CoordinatorSendSerial { message: msg };
                        if let Err(e) = bincode::encode_into_writer(
                            serial_msg.clone(),
                            &mut port_rw,
                            bincode::config::standard(),
                        ) {
                            eprintln!("{:?}", e);
                        }
                    }
                    frostsnap_core::message::CoordinatorSend::ToUser(_) => todo!(),
                }
            }
        } else {
            println!("Did nothing..");
        }
    }
    Ok(())
}
