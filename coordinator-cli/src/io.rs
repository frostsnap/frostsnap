use anyhow::anyhow;
use serialport::SerialPort;
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

pub fn open_device_port(serial_number: &str) -> anyhow::Result<Box<dyn SerialPort>> {
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
    Ok(serialport::new(&port.port_name, 9600)
        .timeout(Duration::from_millis(10))
        .open()?)
}

pub fn wait_for_device_port(serial_number: &str) -> Box<dyn SerialPort> {
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
