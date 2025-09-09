//! Development device binary

#![no_std]
#![no_main]

extern crate alloc;

use esp_hal::entry;

#[entry]
fn main() -> ! {
    // Initialize heap
    esp_alloc::heap_allocator!(256 * 1024);

    // Initialize ESP32 hardware
    let mut peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = esp_hal::clock::CpuClock::max();
        config
    });

    // Bare bones test - no DevicePeripherals, just raw UsbSerialJtag
    {
        use esp_hal::prelude::*;
        use esp_hal::timer::timg::TimerGroup;
        use esp_hal::usb_serial_jtag::UsbSerialJtag;
        use frostsnap_comms::{MAGICBYTES_RECV_DOWNSTREAM, MAGICBYTES_RECV_UPSTREAM};

        // Create USB serial directly
        let mut usb_serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
        
        // Create timer directly
        let timg0 = TimerGroup::new(peripherals.TIMG0);
        let timer = timg0.timer0;

        // Step 1: Read bytes until we find magic bytes (non-blocking with 10s timeout)
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

    unreachable!("here");
    // Unreachable after reset
    loop {}
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
