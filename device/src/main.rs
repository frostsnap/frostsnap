#![no_std]
#![no_main]

#[macro_use]
pub mod device_config;
use crate::device_config::DOUBLE_ENDED;

pub mod uart;

extern crate alloc;
use alloc::string::ToString;
use alloc::vec;
use esp32c3_hal::{
    clock::ClockControl,
    gpio::IO,
    peripherals::Peripherals,
    prelude::*,
    timer::TimerGroup,
    uart::{config, TxRxPins},
    Rtc, Uart,
};
use esp_backtrace as _;
use frostsnap_comms::AnnounceAck;
use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use schnorr_fun::fun::s;
use schnorr_fun::fun::KeyPair;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 320 * 1024;

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
    timer0.start(1u64.secs());

    rtc.swd.disable();
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
    // UART0: display device logs & bootloader stuff
    // UART1: device <--> coordinator communication.

    let mut device_uart = {
        let serial_conf = config::Config {
            baudrate: 9600,
            ..Default::default()
        };
        let txrx0 = TxRxPins::new_tx_rx(
            io.pins.gpio21.into_push_pull_output(),
            io.pins.gpio20.into_floating_input(),
        );
        let serial0 =
            Uart::new_with_config(peripherals.UART0, Some(serial_conf), Some(txrx0), &clocks);
        let device_uart0 = uart::DeviceUart::new(serial0);
        device_uart0
        // if DOUBLE_ENDED {
        //     let txrx0 = TxRxPins::new_tx_rx(
        //         io.pins.gpio14.into_push_pull_output(),
        //         io.pins.gpio15.into_floating_input(),
        //     );
        //     let serial0 =
        //         Uart::new_with_config(peripherals.UART1, Some(serial_conf), Some(txrx0), &clocks);
        //     vec![device_uart1, uart::DeviceUart::new(serial0)]
        // } else {
        //     vec![device_uart1]
        // }
    };
    // let device_uart = device_uarts[0];

    let keypair = KeyPair::new(s!(42));
    let mut frost_device = frostsnap_core::FrostSigner::new(keypair);

    let announce_message = frostsnap_comms::Announce {
        from: frost_device.device_id(),
    };
    let delay = esp32c3_hal::Delay::new(&clocks);
    // TODO: why is announce not continually sending?
    loop {
        delay.delay(3000 as u32);
        // Send announce to coordinator
        match bincode::encode_into_writer(
            announce_message.clone(),
            &mut device_uart,
            bincode::config::standard(),
        ) {
            Err(e) => println!("Error writing announce message: {:?}", e),
            Ok(_) => {
                println!("Announced self");
                let decoded: Result<AnnounceAck, _> =
                    bincode::decode_from_reader(&mut device_uart, bincode::config::standard());

                if let Ok(_) = decoded {
                    println!("Received announce ACK");
                    break;
                }
            }
        }
    }

    bincode::encode_into_writer(
        DeviceSendSerial::Debug("Registered Successfully".to_string()),
        &mut device_uart,
        bincode::config::standard(),
    )
    .unwrap();

    loop {
        let decoded: Result<DeviceReceiveSerial, _> =
            bincode::decode_from_reader(&mut device_uart, bincode::config::standard());

        let mut sends = vec![];
        match decoded {
            Ok(DeviceReceiveSerial { to_device_send }) => {
                // Currently we are assuming all messages received on this layer are intended for us.
                println!("Decoded {:?}", to_device_send);
                sends.extend(
                    frost_device
                        .recv_coordinator_message(to_device_send)
                        .unwrap()
                        .into_iter(),
                );
            }
            Err(e) => {
                match e {
                    bincode::error::DecodeError::LimitExceeded => {
                        // // Wouldblock placeholder
                    }
                    _ => {
                        println!("Decode error: {:?}", e);
                    }
                }
            }
        };

        while !sends.is_empty() {
            let send = sends.pop().unwrap();
            println!("Sending: {:?}", send);
            match send {
                frostsnap_core::message::DeviceSend::ToCoordinator(msg) => {
                    let serial_msg = DeviceSendSerial::Core(msg);
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
                        frostsnap_core::message::DeviceToUserMessage::CheckKeyGen { .. } => {
                            frost_device.keygen_ack(true).unwrap();
                        }
                        frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                            ..
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
