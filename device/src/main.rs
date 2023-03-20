// UART
#![no_std]
#![no_main]

pub mod uart;

extern crate alloc;
use alloc::vec;
use esp32c3_hal::{
    clock::ClockControl, gpio::IO, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rtc,
    Uart,
};
use esp_backtrace as _;
use esp_hal_common::uart::{config, TxRxPins};
use esp_println::println;
use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::DeviceSend;
use schnorr_fun::fun::s;
use schnorr_fun::fun::KeyPair;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 1024;

    extern "C" {
        static mut _heap_start: u32;
    }

    unsafe {
        let heap_start = &_heap_start as *const _ as usize;
        ALLOCATOR.init(heap_start as *mut u8, HEAP_SIZE);
    }
}

#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    // Disable the RTC and TIMG watchdog timers
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks);
    let mut wdt1 = timer_group1.wdt;
    let mut timer0 = timer_group0.timer0;

    rtc.swd.disable();
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    // UART0: display device logs & bootloader stuff
    // UART1: device <--> coordinator communication.
    let txrx = TxRxPins::new_tx_rx(
        io.pins.gpio4.into_push_pull_output(),
        io.pins.gpio5.into_floating_input(),
    );
    let serial = Uart::new_with_config(
        peripherals.UART1,
        Some(config::Config::default()),
        Some(txrx),
        &clocks,
    );

    timer0.start(1u64.secs());
    let mut device_uart = uart::DeviceUart::new(serial, timer0);

    let mut delay = esp32c3_hal::Delay::new(&clocks);

    let keypair = KeyPair::new(s!(42));
    let mut frost_device = frostsnap_core::FrostSigner::new(keypair);

    device_uart.uart.flush().unwrap();
    let mut last_announce_time = 0;
    loop {
        delay.delay_ms(3000 as u32);
        let decoded: Result<DeviceReceiveSerial, _> =
            bincode::decode_from_reader(&mut device_uart, bincode::config::standard());

        let mut sends = match decoded {
            Ok(DeviceReceiveSerial { to_device_send }) => {
                println!("Decoded {:?}", to_device_send);
                frost_device
                    .recv_coordinator_message(to_device_send)
                    .unwrap()
            }
            Err(e) => {
                match e {
                    bincode::error::DecodeError::LimitExceeded => {
                        // Wouldblock placeholder

                        // Announce ourselves if we do fail to decode anything and we are unregistered,
                        match frost_device.announce() {
                            Some(announce) => {
                                vec![DeviceSend::ToCoordinator(announce)]
                            }
                            None => {
                                vec![]
                            }
                        }
                    }
                    _ => {
                        println!("Decode error: {:?}", e);
                        vec![]
                    }
                }
            }
        };

        while !sends.is_empty() {
            let send = sends.pop().unwrap();
            println!("Sending: {:?}", send);
            match send {
                frostsnap_core::message::DeviceSend::ToCoordinator(msg) => {
                    let serial_msg = DeviceSendSerial {
                        message: msg.clone(),
                    };
                    bincode::encode_into_writer(
                        serial_msg.clone(),
                        &mut device_uart,
                        bincode::config::standard(),
                    )
                    .unwrap()
                }
                frostsnap_core::message::DeviceSend::ToUser(message) => {
                    println!("Pretending to get user input for {:?}", message);
                    match message {
                        frostsnap_core::message::DeviceToUserMessage::CheckKeyGen { xpub } => {
                            frost_device.keygen_ack(true).unwrap();
                        }
                        frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                            message_to_sign,
                        } => {
                            let more_sends = frost_device.sign_ack().unwrap();
                            sends.extend(more_sends);
                        }
                    };
                }
            }
        }
    }
    // loop {}
}
