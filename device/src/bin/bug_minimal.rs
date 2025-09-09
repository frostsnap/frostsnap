//! Development device binary

#![no_std]
#![no_main]

extern crate alloc;

use esp_hal::entry;

// Macro to flush TX buffer with timeout
macro_rules! flush_tx {
    ($usb_serial:expr, $timer:expr, $timeout_ms:expr) => {{
        let flush_start = $timer.now();
        loop {
            match $usb_serial.flush_tx_nb() {
                Ok(()) => break,
                Err(nb::Error::WouldBlock) => {
                    if $timer
                        .now()
                        .checked_duration_since(flush_start)
                        .unwrap()
                        .to_millis()
                        > $timeout_ms
                    {
                        panic!("Timeout flushing TX buffer ({}ms)", $timeout_ms);
                    }
                    continue;
                }
                Err(nb::Error::Other(e)) => {
                    panic!("Error flushing TX: {:?}", e);
                }
            }
        }
    }};
}

// Macro to write a byte with timeout
macro_rules! write_timeout {
    ($usb_serial:expr, $timer:expr, $byte:expr, $timeout_ms:expr) => {{
        let write_start = $timer.now();
        loop {
            match $usb_serial.write_byte_nb($byte) {
                Ok(()) => break,
                Err(nb::Error::WouldBlock) => {
                    if $timer
                        .now()
                        .checked_duration_since(write_start)
                        .unwrap()
                        .to_millis()
                        > $timeout_ms
                    {
                        panic!("Timeout writing byte 0x{:02x} ({}ms)", $byte, $timeout_ms);
                    }
                    continue;
                }
                Err(nb::Error::Other(e)) => {
                    panic!("Error writing byte 0x{:02x}: {:?}", $byte, e);
                }
            }
        }
    }};
}

// Macro for busy waiting
macro_rules! busy_wait {
    ($timer:expr, $duration_ms:expr) => {{
        let wait_start = $timer.now();
        while $timer
            .now()
            .checked_duration_since(wait_start)
            .unwrap()
            .to_millis()
            < $duration_ms
        {
            // busy wait
        }
    }};
}

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

        // Create timer directly
        let timg0 = TimerGroup::new(peripherals.TIMG0);
        let timer = timg0.timer0;

        // Step 1: Read bytes until we find magic bytes (non-blocking with 5s timeout between bytes)
        let mut last_read = timer.now();
        let mut received_bytes = alloc::vec::Vec::new();
        let mut usb_serial = UsbSerialJtag::new(peripherals.USB_DEVICE);
        busy_wait!(&timer, 5_000);

        // Write a single 0x00 byte before flushing
        write_timeout!(&mut usb_serial, &timer, 0x00, 1000);

        flush_tx!(&mut usb_serial, &timer, 1001);

        loop {
            // Try to read a byte with timeout tracking
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
                    // Check timeout since last successful read
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
                    continue;
                }
                Err(nb::Error::Other(e)) => {
                    panic!("Error reading byte: {:?}", e);
                }
            }
        }

        // Flush TX buffer even though we haven't written anything. This is a legal operation.
        flush_tx!(&mut usb_serial, &timer, 1000);

        // Step 2: Send magic bytes once (non-blocking with 5s timeout per byte)
        for &byte in &MAGICBYTES_RECV_DOWNSTREAM {
            write_timeout!(&mut usb_serial, &timer, byte, 5000);
        }

        // Flush after writing all bytes
        flush_tx!(&mut usb_serial, &timer, 5000);

        // Step 3: Wait 10 seconds then soft reset
        busy_wait!(&timer, 5_000);
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
