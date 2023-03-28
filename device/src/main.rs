#![no_std]
#![no_main]

#[macro_use]
pub mod device_config;
use crate::device_config::SILENCE_PRINTS;

pub mod uart;

extern crate alloc;
use crate::alloc::string::ToString;
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
use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::DeviceSend;
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
    let mut timer1 = timer_group1.timer0;
    timer1.start(1u64.secs());

    rtc.swd.disable();
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
    // UART0: display device logs & bootloader stuff
    // UART1: device <--> coordinator communication.

    let (mut device_uart0, mut device_uart1) = {
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
        let device_uart0 = uart::DeviceUart::new(serial0, timer0);

        let txrx1 = TxRxPins::new_tx_rx(
            io.pins.gpio4.into_push_pull_output(),
            io.pins.gpio5.into_floating_input(),
        );
        let serial1 =
            Uart::new_with_config(peripherals.UART1, Some(serial_conf), Some(txrx1), &clocks);
        let device_uart1 = uart::DeviceUart::new(serial1, timer1);

        (device_uart0, device_uart1)
    };
    device_uart0.uart.flush().unwrap();
    device_uart1.uart.flush().unwrap();

    // HARDCODED SECRET (MUST BE CHANGED WHEN USING MULTIPLE DEVICES)
    let keypair = KeyPair::new(s!(1243));
    let mut frost_device = frostsnap_core::FrostSigner::new(keypair);

    // Write magic bytes
    if let Err(e) = bincode::encode_into_writer(
        uart::MAGICBYTES,
        &mut device_uart0,
        bincode::config::standard(),
    ) {
        println!("Failed to write magic bytes to UART0");
    }

    let announce_message = DeviceSendSerial::Announce(frostsnap_comms::Announce {
        from: frost_device.device_id(),
    });
    // Send announce to coordinator
    // match bincode::encode_into_writer(
    //     announce_message.clone(),
    //     &mut device_uart0,
    //     bincode::config::standard(),
    // ) {
    //     Err(e) => println!("Error writing announce message: {:?}", e),
    //     Ok(_) => {
    //         println!("Announced self");
    //     }
    // }

    // loop {
    //     let decoded: Result<DeviceReceiveSerial, _> =
    //         bincode::decode_from_reader(&mut device_uart0, bincode::config::standard());
    //     if let Ok(message) = decoded {
    //         if let DeviceReceiveSerial::AnnounceAck(device_id) = message {
    //             if device_id == frost_device.device_id() {
    //                 println!("Received announce ACK");
    //                 break;
    //             }
    //         }
    //     }
    // }

    let mut uart1_active = false;
    let mut sends_uart0 = vec![announce_message];
    let mut sends_uart1 = vec![];
    let mut sends_user = vec![];
    loop {
        if !uart1_active {
            if device_uart1.read_for_magic_bytes() {
                uart1_active = true;
                sends_uart0.push(DeviceSendSerial::Debug {
                    error: "Device {:?} read magic bytes from another device!".to_string(),
                    device: frost_device.device_id(),
                });
            }
        }

        let decoded: Result<DeviceReceiveSerial, _> =
            bincode::decode_from_reader(&mut device_uart0, bincode::config::standard());

        match decoded {
            Ok(received_message) => {
                // Currently we are assuming all messages received on this layer are intended for us.
                println!("Decoded {:?}", received_message);

                match &received_message {
                    DeviceReceiveSerial::AnnounceCoordinator(_) => {
                        if uart1_active {
                            sends_uart1.push(received_message.clone());
                        }
                        sends_uart0.push(DeviceSendSerial::Announce(frostsnap_comms::Announce {
                            from: frost_device.device_id(),
                        }));
                    }
                    DeviceReceiveSerial::AnnounceAck(device_id) => {
                        // Pass on Announce Acks which belong to others
                        if device_id != &frost_device.device_id() {
                            sends_uart1.push(received_message.clone());
                        } else {
                            sends_uart0.push(DeviceSendSerial::Debug {
                                error: "Device {:?} received its registration ACK!".to_string(),
                                device: frost_device.device_id(),
                            });
                        }
                    }
                    DeviceReceiveSerial::Core(core_message) => {
                        if uart1_active {
                            sends_uart1.push(received_message.clone());
                        }

                        match frost_device.recv_coordinator_message(core_message.clone()) {
                            Ok(new_sends) => {
                                for send in new_sends.into_iter() {
                                    match send {
                                        DeviceSend::ToUser(message) => sends_user.push(message),
                                        DeviceSend::ToCoordinator(message) => {
                                            sends_uart0.push(DeviceSendSerial::Core(message))
                                        }
                                    }
                                }
                            }
                            Err(e) => println!("Unexpected FROST message in this state."),
                        }
                        {}
                    }
                }
            }
            Err(e) => {
                match e {
                    bincode::error::DecodeError::LimitExceeded => {}
                    _ => {
                        println!("Decode error: {:?}", e); // TODO "Restarting Message" and restart
                        sends_uart0.push(DeviceSendSerial::Debug {
                            error: "Device {:?} failed to read on UART0: {:?}".to_string(),
                            device: frost_device.device_id(),
                        });
                    }
                }
            }
        };

        if uart1_active {
            let decoded: Result<DeviceSendSerial, _> =
                bincode::decode_from_reader(&mut device_uart1, bincode::config::standard());
            match decoded {
                Ok(device_send) => {
                    println!("Received upstream {:?}", device_send);
                    sends_uart0.push(device_send);
                }
                Err(e) => match e {
                    bincode::error::DecodeError::LimitExceeded => {}
                    _ => {
                        println!("Decode error: {:?}", e);
                        sends_uart0.push(DeviceSendSerial::Debug {
                            error: "Failed to decode on UART0 {:?}".to_string(),
                            device: frost_device.device_id(),
                        });
                    }
                },
            };
        } else {
        }

        // Simulate user keypresses first (TODO: Poll input so we do not hang and delay forwarding)
        for send in sends_user.drain(..) {
            println!("Pretending to get user input for {:?}", send);
            match send {
                frostsnap_core::message::DeviceToUserMessage::CheckKeyGen { .. } => {
                    frost_device.keygen_ack(true).unwrap();
                }
                frostsnap_core::message::DeviceToUserMessage::SignatureRequest { .. } => {
                    let more_sends = frost_device.sign_ack().unwrap();
                    for new_send in more_sends {
                        match new_send {
                            DeviceSend::ToUser(_) => {} // TODO we should never get a second ToUser message from this?
                            DeviceSend::ToCoordinator(send) => {
                                sends_uart0.push(DeviceSendSerial::Core(send))
                            }
                        }
                    }
                }
            };
        }

        for send in sends_uart0.drain(..) {
            println!("Sending: {:?}", send);
            if let Err(e) =
                bincode::encode_into_writer(send, &mut device_uart0, bincode::config::standard())
            {
                println!("Error sending uart0: {:?}", e);
            }
        }

        if uart1_active {
            for send in sends_uart1.drain(..) {
                if let Err(e) = bincode::encode_into_writer(
                    send,
                    &mut device_uart1,
                    bincode::config::standard(),
                ) {
                    println!("Error sending forwarding message: {:?}", e);
                }
            }
        }
    }
    // loop {}
}
