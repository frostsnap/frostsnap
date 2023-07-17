use anyhow::anyhow;
use std::str;
use std::time::Duration;

extern crate alloc;

use crate::serial_rw::SerialPortBincode;

pub fn find_all_ports(usb_id: (u16, u16)) -> impl Iterator<Item = String> {
    let available_ports = serialport::available_ports().unwrap();
    available_ports
        .into_iter()
        .filter_map(move |port| match &port.port_type {
            serialport::SerialPortType::UsbPort(port) => {
                if port.vid == usb_id.0 && port.pid == usb_id.1 {
                    port.serial_number.clone()
                } else {
                    None
                }
            }
            _ => None,
        })
}

pub fn open_device_port(serial_number: &str) -> anyhow::Result<SerialPortBincode> {
    let available_ports = serialport::available_ports().unwrap();
    // println!("Ports: {:?}", available_ports);
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
    let io_port = serialport::new(&port.port_name, 9600)
        .timeout(Duration::from_millis(10))
        .open()?;

    Ok(SerialPortBincode {
        port: io_port,
        serial_number: serial_number.into(),
        buffer: Default::default(),
        next_write_magic: 0,
    })
}

fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("can not read user input");
    let cleaned_input = input.trim().to_string();
    cleaned_input
}

pub fn fetch_input(prompt: &str) -> String {
    println!("{}", prompt);
    read_string()
}
