#![no_std]
#![no_main]

pub mod device_config;
pub mod io;
pub mod oled;
pub mod storage;

#[macro_use]
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
    Delay, Rtc, Uart, UsbSerialJtag,
};
use esp_backtrace as _;
use esp_storage::FlashStorage;
use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::DeviceSend;
use frostsnap_core::schnorr_fun::fun::hex;
use frostsnap_core::schnorr_fun::fun::KeyPair;
use frostsnap_core::schnorr_fun::fun::Scalar;

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

/// # Pin Configuration
///
/// GPIO21:     USB UART0 TX
/// GPIO20:     USB UART0 RX
///
/// GPIO4:      UART1 TX (connect downstream)
/// GPIO5:      UART1 RX (connect downstream)
///
/// RX0:        UART0 RX (connect upstream if not using USB)
/// TX0:        UART0 TX (connect upstream if not using USB)
///
/// GPIO2:      Error LED (optional)
#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let mut system = peripherals.SYSTEM.split();
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

    let mut delay = Delay::new(&clocks);

    let mut button = io.pins.gpio9.into_pull_up_input();
    let wait_button = || {
        // wait for press
        while button.is_high().unwrap() {}
        // wait for release
        while button.is_low().unwrap() {}
    };

    let mut display = oled::SSD1306::new(
        peripherals.I2C0,
        io.pins.gpio5,
        io.pins.gpio6,
        400u32.kHz(),
        &mut system.peripheral_clock_control,
        &clocks,
    )
    .unwrap();
    display.print("frost-esp32").unwrap();

    let flash = FlashStorage::new();
    let mut flash = storage::EspNvs::new(flash, 0x9000);
    // 

    // TODO secure RNG
    let mut rng = esp32c3_hal::Rng::new(peripherals.RNG);
    let mut rand_bytes = [0u8; 32];
    rng.read(&mut rand_bytes).unwrap();
    let secret = Scalar::from_bytes(rand_bytes).unwrap().non_zero().unwrap();
    let keypair = KeyPair::new(secret.clone());

    bincode::encode_into_writer(storage::State{ secret }, flash.rw(), bincode::config::standard()).unwrap();
    let stored: storage::State = bincode::decode_from_reader(flash.rw(), bincode::config::standard()).unwrap();
    println!("read from {:?}", stored);
    // flash
    //     .write_secret(frostsnap_comms::SecretStore { secret })
    //     .unwrap();
    loop {}

    // UART0: display device logs & bootloader stuff
    // UART1: device <--> coordinator communication.
    let jtag = UsbSerialJtag::new(peripherals.USB_DEVICE);

    let (mut upstream_serial, mut downstream_serial) = {
        let serial_conf = config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx0 = TxRxPins::new_tx_rx(
            io.pins.gpio21.into_push_pull_output(),
            io.pins.gpio20.into_floating_input(),
        );
        let uart0 =
            Uart::new_with_config(peripherals.UART0, Some(serial_conf), Some(txrx0), &clocks);

        let upstream_serial = io::BufferedSerialInterface::find_active(uart0, jtag, timer0);
        // let upstream_serial = io::BufferedSerialInterface::new_uart(uart0, timer0);

        let txrx1 = TxRxPins::new_tx_rx(
            io.pins.gpio3.into_push_pull_output(),
            io.pins.gpio4.into_floating_input(),
        );
        let uart1 =
            Uart::new_with_config(peripherals.UART1, Some(serial_conf), Some(txrx1), &clocks);
        let downstream_serial = io::BufferedSerialInterface::new_uart(uart1, timer1);

        (upstream_serial, downstream_serial)
    };
    // upstream_serial.flush().unwrap();
    // downstream_serial.flush().unwrap();

    match upstream_serial.interface {
        io::SerialInterface::Jtag(_) => display.print("Found coordinator").unwrap(),
        io::SerialInterface::Uart(_) => display.print("Found upstream device").unwrap(),
    }

    // Write magic bytes upstream
    if let Err(e) = upstream_serial
        .interface
        .write_bytes(&frostsnap_comms::MAGICBYTES_JTAG)
    {
        println!("Failed to write magic bytes upstream");
        display
            .print("Failed to write magic bytes upstream")
            .unwrap();
    }

    let mut frost_device = frostsnap_core::FrostSigner::new(keypair);

    let announce_message = DeviceSendSerial::Announce(frostsnap_comms::Announce {
        from: frost_device.device_id(),
    });
    let dbg_message = DeviceSendSerial::Debug {
        error: "We sent our announce!!".to_string(),
        device: frost_device.device_id(),
    };

    let mut uart1_active = false;
    let mut sends_uart0 = vec![announce_message, dbg_message];
    let mut sends_uart1 = vec![];
    let mut sends_user = vec![];
    let mut critical_error = false;
    loop {
        if !uart1_active {
            if downstream_serial.read_for_magic_bytes(&frostsnap_comms::MAGICBYTES_UART[..]) {
                uart1_active = true;
                display.print("Found downstream device").unwrap();
                sends_uart0.push(DeviceSendSerial::Debug {
                    error: "Device read magic bytes from another device!".to_string(),
                    device: frost_device.device_id(),
                });
            }
        }

        // Read upstream if there is something to read (from direction of coordinator)
        if upstream_serial.poll_read() {
            let prior_to_read_buff = upstream_serial.read_buffer.clone();
            let decoded: Result<DeviceReceiveSerial, _> =
                bincode::decode_from_reader(&mut upstream_serial, bincode::config::standard());

            match decoded {
                Ok(received_message) => {
                    // Currently we are assuming all messages received on this layer are intended for us.
                    println!("Decoded {:?}", received_message);

                    match &received_message {
                        DeviceReceiveSerial::AnnounceCoordinator(_) => {
                            if uart1_active {
                                sends_uart1.push(received_message.clone());
                            }
                            sends_uart0.push(DeviceSendSerial::Announce(
                                frostsnap_comms::Announce {
                                    from: frost_device.device_id(),
                                },
                            ));
                        }
                        DeviceReceiveSerial::AnnounceAck(device_id) => {
                            // Pass on Announce Acks which belong to others
                            if device_id != &frost_device.device_id() {
                                sends_uart1.push(received_message.clone());
                            } else {
                                display.print("Device registered").unwrap();
                                sends_uart0.push(DeviceSendSerial::Debug {
                                    error: "received registration ACK!".to_string(),
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
                                Err(e) => {
                                    println!("Unexpected FROST message in this state.");
                                    display.print("Unexpected FROST message").unwrap();
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    match e {
                        _ => {
                            println!("Decode error: {:?}", e); // TODO "Restarting Message" and restart
                            display.print(format!("Decode error: {:?}", e)).unwrap();
                            sends_uart0.push(DeviceSendSerial::Debug {
                                error: format!(
                                    "Device failed to read on UART0: {}",
                                    hex::encode(&prior_to_read_buff)
                                ),
                                device: frost_device.device_id(),
                            });
                            critical_error = true;
                        }
                    }
                }
            };
        }

        // Read from downstream if it is active (found magic bytes) and there is something to read
        if uart1_active && downstream_serial.poll_read() {
            let decoded: Result<DeviceSendSerial, _> =
                bincode::decode_from_reader(&mut downstream_serial, bincode::config::standard());
            match decoded {
                Ok(device_send) => {
                    println!("Received upstream {:?}", device_send);
                    sends_uart0.push(device_send);
                }
                Err(e) => match e {
                    _ => {
                        println!("Decode error: {:?}", e);
                        sends_uart0.push(DeviceSendSerial::Debug {
                            error: "Failed to decode on UART0".to_string(),
                            device: frost_device.device_id(),
                        });
                        critical_error = true;
                    }
                },
            };
        }

        // Simulate user keypresses first (TODO: Poll input so we do not hang and delay forwarding)
        for send in sends_user.drain(..) {
            println!("Pretending to get user input for {:?}", send);
            match send {
                frostsnap_core::message::DeviceToUserMessage::CheckKeyGen { xpub } => {
                    // wait_button();
                    frost_device.keygen_ack(true).unwrap();
                    // display.print(format!("{:?}", xpub)).unwrap();
                    display.print("Key generated").unwrap();
                }
                frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                    message_to_sign,
                } => {
                    display
                        .print(format!("Signing: {}", message_to_sign))
                        .unwrap();
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
                bincode::encode_into_writer(send, &mut upstream_serial, bincode::config::standard())
            {
                println!("Error sending uart0: {:?}", e);
            }
        }

        if uart1_active {
            for send in sends_uart1.drain(..) {
                if let Err(e) = bincode::encode_into_writer(
                    send,
                    &mut downstream_serial,
                    bincode::config::standard(),
                ) {
                    println!("Error sending forwarding message: {:?}", e);
                }
            }
        }

        if critical_error {
            break;
        }
    }

    let mut error_led = io.pins.gpio2.into_push_pull_output();
    loop {
        error_led.toggle().unwrap();
        delay.delay_ms(50u32);
    }
}
