#![no_std]
#![no_main]

pub mod device_config;
pub mod io;
pub mod oled;
pub mod state;
pub mod storage;

#[macro_use]
extern crate alloc;
use crate::alloc::string::ToString;
use alloc::vec;
use esp32c3_hal::{
    clock::ClockControl,
    peripherals::Peripherals,
    prelude::*,
    pulse_control::ClockSource,
    timer::TimerGroup,
    uart::{config, TxRxPins},
    Delay, PulseControl, Rtc, Uart, UsbSerialJtag, IO,
};
use esp_backtrace as _;
use esp_hal_smartled::{smartLedAdapter, SmartLedsAdapter};
use esp_storage::FlashStorage;
use smart_leds::{brightness, colors, SmartLedsWrite, RGB};

use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::DeviceSend;
use frostsnap_core::schnorr_fun::fun::hex;
use frostsnap_core::schnorr_fun::fun::marker::Normal;
use frostsnap_core::schnorr_fun::fun::KeyPair;
use frostsnap_core::schnorr_fun::fun::Scalar;
use frostsnap_core::SignerState::FrostKey;

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

    let button = io.pins.gpio9.into_pull_up_input();
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

    // RGB LED
    // White: found coordinator
    // Blue: found another device upstream
    let pulse = PulseControl::new(
        peripherals.RMT,
        &mut system.peripheral_clock_control,
        ClockSource::APB,
        0,
        0,
        0,
    )
    .unwrap();
    let mut led = <smartLedAdapter!(1)>::new(pulse.channel0, io.pins.gpio2);

    // Welcome screen
    display.print("frost-esp32").unwrap();
    for i in 0..=20 {
        led.write([RGB::new(0, i, i)].iter().cloned()).unwrap();
        delay.delay_ms(30u32);
    }
    for i in (0..=20).rev() {
        led.write([RGB::new(0, i, i)].iter().cloned()).unwrap();
        delay.delay_ms(30u32);
    }

    let flash = FlashStorage::new();
    let mut flash = storage::DeviceStorage::new(flash, storage::NVS_PARTITION_START);

    // Simulate factory reset
    // For now we are going to factory reset the storage on boot for easier testing and debugging.
    // Comment out if you want the frost key to persist across reboots
    flash.erase().unwrap();
    // delay.delay_ms(2000u32);

    // Load state from Flash memory if available. If not, generate secret and save.
    let mut device_state: state::DeviceState = match flash.load() {
        Ok(state) => {
            println!("Read device state from flash: {}", state.secret);
            display.print("Read device state from flash").unwrap();
            state
        }
        Err(_e) => {
            // Bincode errored because device is new or something else is wrong,
            // will require manual user interaction to start fresh, or later, restore from backup.
            display
                .print("Press button to generate a new secret")
                .unwrap();
            wait_button();

            let mut rng = esp32c3_hal::Rng::new(peripherals.RNG);
            let mut rand_bytes = [0u8; 32];
            rng.read(&mut rand_bytes).unwrap();
            let secret = Scalar::from_bytes(rand_bytes).unwrap().non_zero().unwrap();

            let state = state::DeviceState {
                secret,
                phase: state::DevicePhase::PreKeygen,
            };
            flash.save(&state).unwrap();
            println!(
                "New secret generated and saved: {}",
                state.secret.to_string()
            );
            display.print("New secret generated and saved").unwrap();
            state
        }
    };

    let keypair = KeyPair::<Normal>::new(device_state.secret.clone());
    // Load the frost signer into the correct state
    let mut frost_signer = match device_state.phase {
        state::DevicePhase::PreKeygen => frostsnap_core::FrostSigner::new(keypair),
        state::DevicePhase::Key { frost_signer } => {
            display
                .print("Loaded existing FROST key from flash!")
                .unwrap();
            frost_signer
        }
    };

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
        io::SerialInterface::Jtag(_) => {
            display.print("Found coordinator").unwrap();
            led.write(brightness([colors::WHITE].iter().cloned(), 10))
                .unwrap();
        }
        io::SerialInterface::Uart(_) => {
            display.print("Found upstream device").unwrap();
            led.write(brightness([colors::BLUE].iter().cloned(), 10))
                .unwrap();
        }
    }

    // Write magic bytes upstream
    if let Err(e) = upstream_serial
        .interface
        .write_bytes(&frostsnap_comms::MAGICBYTES_JTAG)
    {
        println!("Failed to write magic bytes upstream: {:?}", e);
        display
            .print("Failed to write magic bytes upstream")
            .unwrap();
    }

    let announce_message = DeviceSendSerial::Announce(frostsnap_comms::Announce {
        from: frost_signer.device_id(),
    });

    let mut downstream_active = false;
    let mut sends_downstream = vec![announce_message];
    let mut sends_upstream = vec![];
    let mut sends_user = vec![];
    let mut critical_error = false;
    loop {
        // Check if any downstream devices have been connected.
        if !downstream_active {
            if downstream_serial.read_for_magic_bytes(&frostsnap_comms::MAGICBYTES_UART[..]) {
                downstream_active = true;
                display.print("Found downstream device").unwrap();
                sends_downstream.push(DeviceSendSerial::Debug {
                    error: "Device read magic bytes from another device!".to_string(),
                    device: frost_signer.clone().device_id(),
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
                            if downstream_active {
                                sends_upstream.push(received_message.clone());
                            }
                            sends_downstream.push(DeviceSendSerial::Announce(
                                frostsnap_comms::Announce {
                                    from: frost_signer.device_id(),
                                },
                            ));
                        }
                        DeviceReceiveSerial::AnnounceAck(device_id) => {
                            // Pass on Announce Acks which belong to others
                            if device_id != &frost_signer.device_id() {
                                sends_upstream.push(received_message.clone());
                            } else {
                                display.print("Device registered").unwrap();
                                sends_downstream.push(DeviceSendSerial::Debug {
                                    error: "Device received its registration ACK!".to_string(),
                                    device: frost_signer.device_id(),
                                });
                                led.write(brightness([colors::GREEN].iter().cloned(), 10))
                                    .unwrap();
                            }
                        }
                        DeviceReceiveSerial::Core(core_message) => {
                            if downstream_active {
                                sends_upstream.push(received_message.clone());
                            }

                            match frost_signer.recv_coordinator_message(core_message.clone()) {
                                Ok(new_sends) => {
                                    for send in new_sends.into_iter() {
                                        match send {
                                            DeviceSend::ToUser(message) => sends_user.push(message),
                                            DeviceSend::ToCoordinator(message) => sends_downstream
                                                .push(DeviceSendSerial::Core(message)),
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("Unexpected FROST message in this state. {:?}", e);
                                    display
                                        .print(format!("msg!{:?}", received_message))
                                        .unwrap();
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    match e {
                        _ => {
                            println!("Decode error: {:?}", e); // TODO "Restarting Message" and restart
                            display.print(format!("{:?}", e)).unwrap();
                            sends_downstream.push(DeviceSendSerial::Debug {
                                error: format!(
                                    "Device failed to read downstream: {}",
                                    hex::encode(&prior_to_read_buff)
                                ),
                                device: frost_signer.device_id(),
                            });
                            critical_error = true;
                        }
                    }
                }
            };
        }

        // Read from downstream if it is active (found magic bytes) and there is something to read
        if downstream_active && downstream_serial.poll_read() {
            let decoded: Result<DeviceSendSerial, _> =
                bincode::decode_from_reader(&mut downstream_serial, bincode::config::standard());
            match decoded {
                Ok(device_send) => {
                    println!("Received upstream {:?}", device_send);
                    sends_downstream.push(device_send);
                }
                Err(e) => match e {
                    _ => {
                        println!("Decode error: {:?}", e);
                        sends_downstream.push(DeviceSendSerial::Debug {
                            error: "Failed to decode on downstream port".to_string(),
                            device: frost_signer.device_id(),
                        });
                        critical_error = true;
                    }
                },
            };
        }

        // Simulate user keypresses first (TODO: Poll input so we do not hang and delay forwarding)
        for send in sends_user.drain(..) {
            match send {
                frostsnap_core::message::DeviceToUserMessage::CheckKeyGen { xpub } => {
                    led.write(brightness([colors::YELLOW].iter().cloned(), 10))
                        .unwrap();
                    display.print(format!("Key ok?\n{:?}", hex::encode(&xpub.0)));
                    wait_button();
                    frost_signer.keygen_ack(true).unwrap();

                    // STORE FROST KEY INTO FLASH
                    if let FrostKey { key, awaiting_ack } = frost_signer.state() {
                        device_state = state::DeviceState {
                            secret: device_state.secret,
                            phase: state::DevicePhase::Key {
                                frost_signer: frost_signer.clone(),
                            },
                        };
                        flash.save(&device_state).unwrap();
                    }
                    display.print(format!("Key generated\n{:?}", hex::encode(&xpub.0)));
                    led.write(brightness([colors::BLUE].iter().cloned(), 10))
                        .unwrap();
                }
                frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                    message_to_sign,
                } => {
                    display
                        .print(format!("Sign?\n{}", message_to_sign))
                        .unwrap();
                    led.write(brightness([colors::YELLOW].iter().cloned(), 10))
                        .unwrap();
                    wait_button();
                    let more_sends = frost_signer.sign_ack().unwrap();
                    led.write(brightness([colors::BLUE].iter().cloned(), 10))
                        .unwrap();
                    display
                        .print(format!("Sending signature\n{}", message_to_sign))
                        .unwrap();
                    for new_send in more_sends {
                        match new_send {
                            DeviceSend::ToUser(_) => {}
                            DeviceSend::ToCoordinator(send) => {
                                sends_downstream.push(DeviceSendSerial::Core(send))
                            }
                        }
                    }
                }
            };
        }

        for send in sends_downstream.drain(..) {
            println!("Sending: {:?}", send);
            if let Err(e) =
                bincode::encode_into_writer(send, &mut upstream_serial, bincode::config::standard())
            {
                println!("Error sending downstream: {:?}", e);
            }
        }

        if downstream_active {
            for send in sends_upstream.drain(..) {
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

    let mut i = 0;
    loop {
        i += 1;
        led.write([RGB::new((i % 20) + 10, 0, 0)].iter().cloned())
            .unwrap();
        delay.delay_ms(30u32);
    }
}
