use frostsnap_core::message::CoordinatorToDeviceMessage;
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

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
struct FrostMessage {
    message: String,
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

    let write_message = FrostMessage {
        message: "ayoo".to_string(),
    };

    loop {
        // std::thread::sleep(Duration::from_millis(1000));
        let choice = fetch_input("\nPress:\n\tr - read\n\tw - write\n");
        if choice == "w" {
            if let Err(e) = bincode::encode_into_writer(
                write_message.clone(),
                &mut port_rw,
                bincode::config::standard(),
            ) {
                eprintln!("{:?}", e);
            }
        } else if choice == "r" {
            let decode: Result<FrostMessage, _> =
                bincode::decode_from_reader(&mut port_rw, bincode::config::standard());
            match decode {
                Ok(msg) => println!("Read: {:?}", msg),
                Err(e) => {
                    eprintln!("{:?}", e);
                }
            }
        }
    }
    Ok(())
}
