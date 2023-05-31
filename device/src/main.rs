#![no_std]
#![no_main]

pub mod buttons;
pub mod device_config;
pub mod io;
pub mod st7735;
pub mod state;
pub mod storage;

#[macro_use]
extern crate alloc;
use crate::alloc::string::ToString;
use alloc::{collections::VecDeque, string::String, vec::Vec};
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
use esp_storage::FlashStorage;
use frostsnap_comms::{
    DeviceReceiveBody, DeviceReceiveSerial, DeviceSendMessage, DeviceSendSerial,
};
use frostsnap_comms::{DeviceReceiveMessage, Downstream};
use frostsnap_core::message::{CoordinatorToDeviceMessage, DeviceSend, DeviceToUserMessage};
use io::UpstreamDetector;

use buttons::ButtonDirection;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_framebuf::FrameBuf;

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

    // construct the 5-position button on the Air101 LCD board
    // orientation: usb-c port on the right
    // down button shares same pin as D5 LED, which pulls the input down enough to cause problems.
    // remove the LED
    let mut buttons = buttons::Buttons::new(
        io.pins.gpio4,
        io.pins.gpio8,
        io.pins.gpio13,
        io.pins.gpio9,
        io.pins.gpio5,
    );

    let mut bl = io.pins.gpio11.into_push_pull_output();
    // Turn off backlight to hide artifacts as display initializes
    bl.set_low().unwrap();
    let mut framearray = [Rgb565::WHITE; 160 * 80];
    let framebuf = FrameBuf::new(&mut framearray, 160, 80);
    let mut display = st7735::ST7735::new(
        // &mut bl,
        io.pins.gpio6.into_push_pull_output(),
        io.pins.gpio10.into_push_pull_output(),
        peripherals.SPI2,
        io.pins.gpio2,
        io.pins.gpio7,
        io.pins.gpio3,
        io.pins.gpio12,
        &mut system.peripheral_clock_control,
        &clocks,
        framebuf,
    )
    .unwrap();

    let flash = FlashStorage::new();
    let mut flash = storage::DeviceStorage::new(flash, storage::NVS_PARTITION_START);

    // Welcome screen
    // Some delay before turning on backlight to hide screen flicker
    delay.delay_ms(20u32);
    bl.set_high().unwrap();
    display.splash_screen().unwrap();
    display.clear(Rgb565::BLACK).unwrap();
    display.header("frostsnap").unwrap();
    display.flush().unwrap();

    // Simulate factory reset
    // For now we are going to factory reset the storage on boot for easier testing and debugging.
    // Comment out if you want the frost key to persist across reboots
    // flash.erase().unwrap();
    // delay.delay_ms(2000u32);

    // Load state from Flash memory if available. If not, generate secret and save.
    let mut frost_signer = match flash.load() {
        Ok(state) => {
            display
                .print(format!("STATE: {:?}", state.signer.state()))
                .unwrap();
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
    let mut sends_downstream: Vec<DeviceReceiveMessage> = vec![];
    let mut sends_upstream: Vec<DeviceSendMessage> = vec![];
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
            sends_upstream = vec![DeviceSendMessage::Announce(frostsnap_comms::Announce {
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
                                DeviceSendMessage::Debug {
                                    message: format!(
                                        "downstream device sent unexpected magic bytes"
                                    ),
                                    device: frost_signer.device_id(),
                                }
                            }
                            DeviceSendSerial::Message(message) => match message {
                                DeviceSendMessage::Core(core) => DeviceSendMessage::Core(core),
                                DeviceSendMessage::Debug { message, device } => {
                                    DeviceSendMessage::Debug { message, device }
                                }
                                DeviceSendMessage::Announce(message) => {
                                    DeviceSendMessage::Announce(message)
                                }
                            },
                        };
                        sends_upstream.push(forward_upstream);
                    }
                    Err(e) => {
                        sends_upstream.push(DeviceSendMessage::Debug {
                            message: format!("Failed to decode on downstream port: {e}"),
                            device: frost_signer.device_id(),
                        });
                        downstream_active = false;
                    }
                };
            }

            // Send messages downstream
            for send in sends_downstream.drain(..) {
                downstream_serial
                    .forward_downstream(DeviceReceiveSerial::Message(send))
                    .expect("sending downstream");
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
                sends_upstream.push(DeviceSendMessage::Debug {
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

            display
                .print(format!("Waiting for coordinator {scanning}",))
                .unwrap();
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
                        match received_message {
                            DeviceReceiveSerial::MagicBytes(_) => {
                                if upstream_received_first_message {
                                    soft_reset = true;
                                }
                                continue;
                            }
                            DeviceReceiveSerial::Message(message) => {
                                // We have recieved a first message (if this is not a magic bytes message)
                                upstream_received_first_message = true;
                                // Forward messages downstream if there are other target destinations
                                if downstream_active {
                                    let mut forwarding_message = message.clone();
                                    let _ = forwarding_message
                                        .target_destinations
                                        .remove(&frost_signer.device_id());
                                    if forwarding_message.target_destinations.len() > 0 {
                                        sends_downstream.push(forwarding_message);
                                    }
                                }
                                // Skip processing of messages which are not destined for us
                                if !message
                                    .target_destinations
                                    .contains(&frost_signer.device_id())
                                {
                                    continue;
                                }

                                match message.message_body {
                                    DeviceReceiveBody::AnnounceAck { device_label, .. } => {
                                        display.print_header(&device_label).unwrap();
                                        display.header(device_label).unwrap();
                                        display.flush().unwrap();
                                        sends_upstream.push(DeviceSendMessage::Debug {
                                            message: "Received AnnounceACK!".to_string(),
                                            device: frost_signer.device_id(),
                                        });
                                    }
                                    DeviceReceiveBody::Core(core_message) => {
                                        if let CoordinatorToDeviceMessage::DoKeyGen {
                                            devices,
                                            ..
                                        } = &core_message
                                        {
                                            if devices.contains(&frost_signer.device_id()) {
                                                frost_signer.clear_state();
                                            }
                                        }

                                        match frost_signer
                                            .recv_coordinator_message(core_message.clone())
                                        {
                                            Ok(new_sends) => {
                                                outbox.extend(new_sends);
                                            }
                                            Err(e) => {
                                                println!(
                                                    "Unexpected FROST message in this state. {:?}",
                                                    e
                                                );
                                                display.print(e.gist()).unwrap();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        sends_upstream.push(DeviceSendMessage::Debug {
                            message: format!(
                                "Device failed to read upstream: {}",
                                hex::encode(&prior_to_read_buff)
                            ),
                            device: frost_signer.device_id(),
                        });
                        panic!("upstream read fail: {}", hex::encode(&prior_to_read_buff));
                    }
                };
            }

            for send in sends_upstream.drain(..) {
                upstream_serial
                    .send_to_coodinator(DeviceSendSerial::Message(send.clone()))
                    .expect("unable to send to coordinator");
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
                }
                DeviceSend::ToCoordinator(message) => {
                    sends_upstream.push(DeviceSendMessage::Core(message));
                }
                DeviceSend::ToUser(user_send) => {
                    match user_send {
                        DeviceToUserMessage::CheckKeyGen { xpub } => {
                            // display.print(format!("Key ok?\n{:?}", hex::encode(&xpub.0)));
                            // wait_button();
                            outbox.extend(frost_signer.keygen_ack(true).unwrap());
                            display.print(format!("Key generated\n{}", xpub)).unwrap();
                            delay.delay_ms(2_000u32);
                        }
                        frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                            message_to_sign,
                            ..
                        } => {
                            let mut choice = true;
                            loop {
                                display.confirm_view(format!("Sign {}", message_to_sign), choice).unwrap();

                                match buttons.wait_for_press() {
                                    ButtonDirection::Center => break,
                                    ButtonDirection::Left => {
                                        choice = false;
                                    }
                                    ButtonDirection::Right => {
                                        choice = true;
                                    }
                                    _ => {}
                                }
                            }

                            if choice {
                                outbox.extend(frost_signer.sign_ack().unwrap());
                                display.print("Request to sign accepted").unwrap();
                            } else {
                                display.print("Request to sign rejected").unwrap();
                            }
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

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

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

    let mut framearray = [Rgb565::WHITE; 160 * 80];
    let framebuf = FrameBuf::new(&mut framearray, 160, 80);
    // let mut bl = io.pins.gpio11.into_push_pull_output();
    if let Ok(mut display) = st7735::ST7735::new(
        // &mut bl,
        io.pins.gpio6.into_push_pull_output(),
        io.pins.gpio10.into_push_pull_output(),
        peripherals.SPI2,
        io.pins.gpio2,
        io.pins.gpio7,
        io.pins.gpio3,
        io.pins.gpio12,
        &mut system.peripheral_clock_control,
        &clocks,
        framebuf,
    ) {
        let _ = display.error_print(message);
    }

    loop {}
}
