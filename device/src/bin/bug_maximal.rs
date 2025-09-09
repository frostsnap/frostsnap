//! Development device binary

#![no_std]
#![no_main]

extern crate alloc;

use core::cell::RefCell;
use esp_hal::entry;
use esp_storage::FlashStorage;
use frostsnap_device::{esp32_run, peripherals::DevicePeripherals, resources::Resources};

#[entry]
fn main() -> ! {
    // Initialize heap
    esp_alloc::heap_allocator!(256 * 1024);

    // Initialize ESP32 hardware
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = esp_hal::clock::CpuClock::max();
        config
    });

    // Initialize flash storage (must stay alive for partition references)
    let flash = RefCell::new(FlashStorage::new());

    // Initialize all device peripherals with initial RNG
    let mut device = DevicePeripherals::init(peripherals);

    // Test with DevicePeripherals initialized (will reset before reaching normal code)
    {
        use esp_hal::prelude::*;
        use frostsnap_comms::{MAGICBYTES_RECV_DOWNSTREAM, MAGICBYTES_RECV_UPSTREAM};

        // Use the USB serial from DevicePeripherals
        let usb_serial = &mut device.jtag;
        
        // Use the timer from DevicePeripherals
        let timer = device.timer;

        // Step 1: Read bytes until we find magic bytes (non-blocking with timeout)
        let mut last_read = timer.now();
        let mut received_bytes = alloc::vec::Vec::new();
        loop {
            match usb_serial.read_byte() {
                Ok(byte) => {
                    last_read = timer.now();
                    received_bytes.push(byte);

                    // Check if we have the magic bytes at the end
                    if received_bytes.len() >= 7 {
                        let start = received_bytes.len() - 7;
                        if &received_bytes[start..] == &MAGICBYTES_RECV_UPSTREAM {
                            break; // Found magic bytes!
                        }
                    }

                    // Prevent unbounded growth
                    if received_bytes.len() > 100 {
                        received_bytes.drain(0..50);
                    }
                }
                Err(nb::Error::WouldBlock) => {
                    // Check timeout
                    if timer
                        .now()
                        .checked_duration_since(last_read)
                        .unwrap()
                        .to_millis()
                        > 5_000
                    {
                        panic!(
                            "Timeout reading magic bytes! Read {} bytes total",
                            received_bytes.len()
                        );
                    }
                    // Try again
                    continue;
                }
                Err(nb::Error::Other(e)) => {
                    panic!("Error reading byte: {:?}", e);
                }
            }
        }
        
        // Step 2: Send magic bytes once (non-blocking with 5s timeout)
        let write_start = timer.now();
        let mut bytes_written = 0;

        for &byte in &MAGICBYTES_RECV_DOWNSTREAM {
            loop {
                match usb_serial.write_byte_nb(byte) {
                    Ok(()) => {
                        bytes_written += 1;
                        break;
                    }
                    Err(nb::Error::WouldBlock) => {
                        // Check timeout
                        if timer
                            .now()
                            .checked_duration_since(write_start)
                            .unwrap()
                            .to_millis()
                            > 5000
                        {
                            panic!(
                                "Timeout writing magic bytes! Only wrote {} bytes. NG",
                                bytes_written
                            );
                        }
                        // Try again
                        continue;
                    }
                    Err(nb::Error::Other(e)) => {
                        panic!("Error writing byte: {:?}", e);
                    }
                }
            }
        }

        // Flush with timeout
        let flush_start = timer.now();
        loop {
            match usb_serial.flush_tx_nb() {
                Ok(()) => break,
                Err(nb::Error::WouldBlock) => {
                    if timer
                        .now()
                        .checked_duration_since(flush_start)
                        .unwrap()
                        .to_millis()
                        > 5000
                    {
                        panic!("Timeout flushing TX buffer after writing {} bytes", bytes_written);
                    }
                    continue;
                }
                Err(nb::Error::Other(e)) => {
                    panic!("Error flushing TX: {:?}", e);
                }
            }
        }

        // Step 3: Wait 4 seconds then soft reset
        let final_wait_start = timer.now();
        while timer
            .now()
            .checked_duration_since(final_wait_start)
            .unwrap()
            .to_millis()
            < 4000
        {
            // busy wait
        }
        esp_hal::reset::software_reset();
    }

    // UNREACHABLE CODE BELOW - but included for compilation
    // Check if the device needs provisioning
    if device.needs_factory_provisioning() {
        // Run dev provisioning - this will reset the device
        frostsnap_device::factory::run_dev_provisioning(device);
    } else {
        // Device is already provisioned - proceed with normal boot
        let mut resources = Resources::init_dev(device, &flash);

        // Run main event loop
        esp32_run::run(&mut resources);
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
