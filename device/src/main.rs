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
use alloc::{collections::VecDeque, vec::Vec};
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
use io::UpstreamDetector;
use smart_leds::{brightness, colors, SmartLedsWrite, RGB};

use frostsnap_comms::{Downstream, Upstream};
use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial};
use frostsnap_core::message::{DeviceSend, DeviceToUserMessage};
use frostsnap_core::schnorr_fun::fun::hex;
use frostsnap_core::schnorr_fun::fun::marker::Normal;
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
/// GPIO21:     USB UART0 TX  (connect upstream)
/// GPIO20:     USB UART0 RX  (connect upstream)
///
/// GPIO18:     JTAG/UART1 TX (connect downstream)
/// GPIO19:     JTAG/UART1 RX (connect downstream)
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

    // let button = io.pins.gpio9.into_pull_up_input();
    // let wait_button = || {
    //     // Ensure button is not pressed
    //     while button.is_high().unwrap() {}
    //     // Wait for press
    //     while button.is_low().unwrap() {}
    // };

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

    let flash = FlashStorage::new();
    let mut flash = storage::DeviceStorage::new(flash, storage::NVS_PARTITION_START);

    // Welcome screen
    display.print_header("frost snap").unwrap();
    for i in 0..=20 {
        led.write([RGB::new(0, i, i)].iter().cloned()).unwrap();
        delay.delay_ms(30u32);
    }
    for i in (0..=20).rev() {
        led.write([RGB::new(0, i, i)].iter().cloned()).unwrap();
        delay.delay_ms(30u32);
    }

    // Simulate factory reset
    // For now we are going to factory reset the storage on boot for easier testing and debugging.
    // Comment out if you want the frost key to persist across reboots
    // flash.erase().unwrap();
    // delay.delay_ms(2000u32);

    // Load state from Flash memory if available. If not, generate secret and save.
    let mut frost_signer = match flash.load() {
        Ok(state) => {
            display.print(format!("STATE: {:?}", state.signer.state())).unwrap();
            delay.delay_ms(1_000u32);

            state.signer
        }
        Err(e) => {
            // Bincode errored because device is new or something else is wrong,
            // will require manual user interaction to start fresh, or later, restore from backup.
            // display
            //     .print("Press button to generate a new secret")
            //     .unwrap();
            // wait_button();
            display.print(e.to_string()).unwrap();
            delay.delay_ms(2_000u32);

            let mut rng = esp32c3_hal::Rng::new(peripherals.RNG);
            let mut rand_bytes = [0u8; 32];
            rng.read(&mut rand_bytes).unwrap();
            let secret = Scalar::from_bytes(rand_bytes).unwrap().non_zero().unwrap();
            let keypair: KeyPair = KeyPair::<Normal>::new(secret.clone());
            let frost_signer = frostsnap_core::FrostSigner::new(keypair);

            flash
                .save(&state::FrostState {
                    signer: frost_signer.clone(),
                })
                .unwrap();
            println!("New secret generated and saved: {}", secret.to_string());
            display.print("New secret generated and saved").unwrap();
            frost_signer
        }
    };

    delay.delay_ms(1_000u32);

    let mut downstream_serial = {
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
        io::SerialInterface::<_, _, Downstream>::new_uart(uart0, &timer1)
    };

    let upstream_uart = {
        let serial_conf = config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx1 = TxRxPins::new_tx_rx(
            io.pins.gpio18.into_push_pull_output(),
            io.pins.gpio19.into_floating_input(),
        );
        Uart::new_with_config(peripherals.UART1, Some(serial_conf), Some(txrx1), &clocks)
    };
    let upstream_jtag = UsbSerialJtag::new(peripherals.USB_DEVICE);

    let mut soft_reset = true;
    let mut downstream_active = false;
    let mut sends_downstream: Vec<DeviceReceiveSerial<Downstream>> = vec![];
    let mut sends_upstream: Vec<DeviceSendSerial<Upstream>> = vec![];
    let mut sends_user: Vec<DeviceToUserMessage> = vec![];
    let mut outbox = VecDeque::new();
    let mut upstream_detector = UpstreamDetector::new(upstream_uart, upstream_jtag, &timer0);
    let mut upstream_sent_magic_bytes = false;
    let mut upstream_received_first_message = false;
    let mut next_write_magic_bytes = 0;

    loop {
        if soft_reset {
            display.print("soft resetting").unwrap();
            delay.delay_ms(500u32);
            soft_reset = false;
            sends_upstream = vec![DeviceSendSerial::Announce(frostsnap_comms::Announce {
                from: frost_signer.device_id(),
            })];
            sends_user.clear();
            sends_downstream.clear();
            downstream_active = false;
            upstream_sent_magic_bytes = false;
            next_write_magic_bytes = 0;
            upstream_received_first_message = false;
            outbox.clear();
        }

        if downstream_active {
            if downstream_serial.poll_read() {
                match downstream_serial.receive_from_downstream() {
                    Ok(device_send) => {
                        let forward_upstream = match device_send {
                            DeviceSendSerial::MagicBytes(_) => {
                                // soft reset downstream if it sends unexpected magic bytes so we restablish
                                // downstream_active = false;
                                DeviceSendSerial::Debug { message: format!("downstream device sent unexpected magic bytes"), device: frost_signer.device_id() }
                            },
                            DeviceSendSerial::Core(core) => DeviceSendSerial::Core(core),
                            DeviceSendSerial::Debug { message, device } => DeviceSendSerial::Debug { message, device },
                            DeviceSendSerial::Announce(message) => DeviceSendSerial::Announce(message),
                        };
                        sends_upstream.push(forward_upstream);
                    }
                    Err(e) => {
                        sends_upstream.push(DeviceSendSerial::Debug {
                            message: format!("Failed to decode on downstream port: {e}"),
                            device: frost_signer.device_id(),
                        });
                        downstream_active = false;
                    }
                };
            }

            // Send messages downstream
            for send in sends_downstream.drain(..) {
                downstream_serial.forward_downstream(send).expect("sending downstream");
            }
        } else {
            let now = timer0.now();
            if now > next_write_magic_bytes {
                next_write_magic_bytes = now + 40_000 * 100;
                // display.print("writing magic bytes downstream").unwrap();
                downstream_serial
                    .write_magic_bytes()
                    .expect("couldn't write magic bytes downstream");
            }
            if downstream_serial.find_and_remove_magic_bytes() {
                downstream_active = true;
                sends_upstream.push(DeviceSendSerial::Debug {
                    message: "Device read magic bytes from another device!".to_string(),
                    device: frost_signer.clone().device_id(),
                });
            }
        }

        if upstream_detector.serial_interface().is_none() {
            let scanning = if upstream_detector.switched {
                "JTAG"
            } else {
                "UART"
            };

            display.print(format!("Waiting for coordinator {scanning}",)) .unwrap();
        }


        if let Some(upstream_serial) = upstream_detector.serial_interface() {
            if !upstream_sent_magic_bytes {
                upstream_serial
                    .write_magic_bytes()
                    .expect("failed to write magic bytes");
                display.print("Waiting for coordinator").unwrap();
                upstream_sent_magic_bytes = true;
            }

            if upstream_serial.poll_read() {
                let prior_to_read_buff = upstream_serial.read_buffer().to_vec();

                match upstream_serial.receive_from_coordinator() {
                    Ok(received_message) => {
                        if !matches!(&received_message, DeviceReceiveSerial::MagicBytes(_)) {
                            upstream_received_first_message = true;
                        }
                        match received_message {
                            DeviceReceiveSerial::MagicBytes(_) => {
                                if upstream_received_first_message {
                                    soft_reset = true;
                                }
                                continue;
                            }
                            DeviceReceiveSerial::AnnounceAck {
                                device_id,
                                device_label,
                            } => {
                                // Pass on Announce Acks which belong to others
                                if device_id != frost_signer.device_id() {
                                    sends_downstream.push(DeviceReceiveSerial::AnnounceAck {
                                        device_id,
                                        device_label,
                                    });
                                } else {
                                    display.print_header(device_label).unwrap();
                                    sends_upstream.push(DeviceSendSerial::Debug {
                                        message: "Received AnnounceACK!".to_string(),
                                        device: frost_signer.device_id(),
                                    });
                                    led.write(brightness([colors::GREEN].iter().cloned(), 10))
                                        .unwrap();
                                }
                            }
                            DeviceReceiveSerial::Core(core_message) => {
                                if downstream_active {
                                    sends_downstream
                                        .push(DeviceReceiveSerial::Core(core_message));
                                }
                                if let frostsnap_core::message::CoordinatorToDeviceMessage::DoKeyGen {
                                    devices,
                                    ..
                                } = &core_message
                                {
                                    if devices.contains(&frost_signer.device_id()) {
                                        frost_signer.clear_state();
                                    }
                                }

                                match frost_signer.recv_coordinator_message(core_message.clone()) {
                                    Ok(new_sends) => {
                                        outbox.extend(new_sends);

                                    }
                                    Err(e) => {
                                        println!("Unexpected FROST message in this state. {:?}", e);
                                        display.print(&e.gist()).unwrap();
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        sends_upstream.push(DeviceSendSerial::Debug {
                            message: format!(
                                "Device failed to read upstream: {}",
                                hex::encode(&prior_to_read_buff)
                            ),
                            device: frost_signer.device_id(),
                        });
                        panic!( "upstream read fail: {}", hex::encode(&prior_to_read_buff));
                    }
                };
            }

            for send in sends_upstream.drain(..) {
                upstream_serial.send_to_coodinator(send.clone()).expect("unable to send to coordinator");
            }
        }

        // Handle message outbox to send: ToStorage, ToCoordinator, ToUser.
        // ⚠ pop_front ensures messages are sent in order. E.g. update nonce NVS before sending sig.
        while let Some(send) = outbox.pop_front() {
            match send {
                DeviceSend::ToStorage(_) => {
                    delay.delay_ms(2_000u32);
                    flash
                        .save(&state::FrostState {
                            signer: frost_signer.clone(),
                        })
                        .unwrap();
                    led.write(brightness([colors::BLUE].iter().cloned(), 10))
                        .unwrap();
                }
                DeviceSend::ToCoordinator(message) => {
                    sends_upstream.push(DeviceSendSerial::Core(message));
                }
                DeviceSend::ToUser(user_send) => {
                    match user_send {
                        frostsnap_core::message::DeviceToUserMessage::CheckKeyGen { xpub } => {
                            led.write(brightness([colors::YELLOW].iter().cloned(), 10))
                                .unwrap();
                            // display.print(format!("Key ok?\n{:?}", hex::encode(&xpub.0)));
                            // wait_button();
                            outbox.extend(frost_signer.keygen_ack(true).unwrap());
                            display
                                .print(format!("Key generated\n{:?}", hex::encode(&xpub.0)))
                                .unwrap();
                            led.write(brightness([colors::WHITE_SMOKE].iter().cloned(), 10))
                                .unwrap();
                            delay.delay_ms(2_000u32);
                        }
                        frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                            messages_to_sign,
                        } => {
                            display
                                .print(format!("Signing\n{:?}", messages_to_sign))
                                .unwrap();
                            led.write(brightness([colors::FUCHSIA].iter().cloned(), 10))
                                .unwrap();
                            outbox.extend(frost_signer.sign_ack().unwrap());
                        }
                    };
                }
            }
        }

    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    let mut system = peripherals.SYSTEM.split();
    // Disable the RTC and TIMG watchdog timers

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
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut led = <smartLedAdapter!(1)>::new(pulse.channel0, io.pins.gpio2);
    led.write(brightness([colors::RED].iter().cloned(), 10))
        .unwrap();

    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    let message = match info.location() {
        Some(location) => format!(
            "{}:{} {}",
            location.file().split('/').last().unwrap_or(""),
            location.line(),
            info.to_string()
        ),
        None => info.to_string(),
    };

    if let Ok(mut display) = oled::SSD1306::new(
        peripherals.I2C0,
        io.pins.gpio5,
        io.pins.gpio6,
        400u32.kHz(),
        &mut system.peripheral_clock_control,
        &clocks,
    ) {
        let _ = display.print(message);
    }

    loop {}
}
