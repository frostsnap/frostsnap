#![no_std]
#![no_main]

pub mod device_config;
pub mod device_interface;
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

use crate::device_interface::Device;

use frostsnap_comms::{
    DeviceReceiveBody, DeviceReceiveSerial, DeviceSendMessage, DeviceSendSerial,
};
use frostsnap_comms::{DeviceReceiveMessage, Downstream};
use frostsnap_core::message::{CoordinatorToDeviceMessage, DeviceSend, DeviceToUserMessage};
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

    let device = device_interface::PurpleDevice::new();

    // Load state from Flash memory if available. If not, generate secret and save.
    let mut frost_signer = match device.flash_load() {
        Ok(state) => {
            device
                .print(format!("STATE: {:?}", state.signer.state()))
                .unwrap();
            device.delay_ms(1_000u32);

            state.signer
        }
        Err(e) => {
            // Bincode errored because device is new or something else is wrong,
            // will require manual user interaction to start fresh, or later, restore from backup.
            // device
            //     .print("Press button to generate a new secret")
            //     .unwrap();
            // wait_button();
            device.print(e.to_string()).unwrap();
            device.delay_ms(2_000u32);

            let mut rand_bytes = [0u8; 32];
            device.read_rng(&mut rand_bytes).unwrap();
            let secret = Scalar::from_bytes(rand_bytes).unwrap().non_zero().unwrap();
            let keypair: KeyPair = KeyPair::<Normal>::new(secret.clone());
            let frost_signer = frostsnap_core::FrostSigner::new(keypair);

            device
                .flash_save(&state::FrostState {
                    signer: frost_signer.clone(),
                })
                .unwrap();
            println!("New secret generated and saved: {}", secret.to_string());
            device.print("New secret generated and saved").unwrap();
            frost_signer
        }
    };

    // Welcome screen
    device.print_header("frost snap").unwrap();
    for i in 0..=20 {
        device.led_write(RGB::new(0, i, i)).unwrap();
        device.delay_ms(30u32);
    }
    for i in (0..=20).rev() {
        device.led_write(RGB::new(0, i, i)).unwrap();
        device.delay_ms(30u32);
    }

    let mut soft_reset = true;
    let mut downstream_active = false;
    let mut sends_downstream: Vec<DeviceReceiveMessage> = vec![];
    let mut sends_upstream: Vec<DeviceSendMessage> = vec![];
    let mut sends_user: Vec<DeviceToUserMessage> = vec![];
    let mut outbox = VecDeque::new();
    // let mut device.upstream_detector = UpstreamDetector::new(upstream_uart, upstream_jtag, &timer0);
    let mut upstream_sent_magic_bytes = false;
    let mut upstream_received_first_message = false;
    let mut next_write_magic_bytes = 0;

    loop {
        if soft_reset {
            device.print("soft resetting").unwrap();
            device.delay_ms(500u32);
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
            if device.poll_read_downstream() {
                match device.receive_from_downstream() {
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
                device
                    .downstream_serial
                    .forward_downstream(DeviceReceiveSerial::Message(send))
                    .expect("sending downstream");
            }
        } else {
            let now = device.now();
            if now > next_write_magic_bytes {
                next_write_magic_bytes = now + 40_000 * 100;
                // device.print("writing magic bytes downstream").unwrap();
                device
                    .downstream_serial
                    .write_magic_bytes()
                    .expect("couldn't write magic bytes downstream");
            }
            if device.downstream_serial.find_and_remove_magic_bytes() {
                downstream_active = true;
                sends_upstream.push(DeviceSendMessage::Debug {
                    message: "Device read magic bytes from another device!".to_string(),
                    device: frost_signer.clone().device_id(),
                });
            }
        }

        if device.upstream_serial_interface().is_none() {
            let scanning = if device.upstream_serial_detector_status() {
                "JTAG"
            } else {
                "UART"
            };

            device
                .print(format!("Waiting for coordinator {scanning}",))
                .unwrap();
        }

        if let Some(upstream_serial) = device.upstream_serial_interface() {
            if !upstream_sent_magic_bytes {
                upstream_serial
                    .write_magic_bytes()
                    .expect("failed to write magic bytes");
                device.print("Waiting for coordinator").unwrap();
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
                                        device.print_header(&device_label).unwrap();
                                        sends_upstream.push(DeviceSendMessage::Debug {
                                            message: "Received AnnounceACK!".to_string(),
                                            device: frost_signer.device_id(),
                                        });
                                        device.led_write(RGB::new(0, 255, 0)).unwrap();
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
                                                device.print(&e.gist()).unwrap();
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
                    device.delay_ms(2_000u32);
                    device
                        .flash_save(&state::FrostState {
                            signer: frost_signer.clone(),
                        })
                        .unwrap();
                    device.led_write(RGB::new(0, 0, 255)).unwrap();
                }
                DeviceSend::ToCoordinator(message) => {
                    sends_upstream.push(DeviceSendMessage::Core(message));
                }
                DeviceSend::ToUser(user_send) => {
                    match user_send {
                        DeviceToUserMessage::CheckKeyGen { xpub } => {
                            device.led_write(RGB::new(255, 255, 0)).unwrap();
                            // device.print(format!("Key ok?\n{:?}", hex::encode(&xpub.0)));
                            // wait_button();
                            outbox.extend(frost_signer.keygen_ack(true).unwrap());
                            device.print(&format!("Key generated\n{}", xpub)).unwrap();
                            device.led_write(RGB::new(255, 255, 255)).unwrap();
                            device.delay_ms(2_000u32);
                        }
                        frostsnap_core::message::DeviceToUserMessage::SignatureRequest {
                            message_to_sign,
                            ..
                        } => {
                            device.print(&format!("Sign {}", message_to_sign)).unwrap();
                            device.led_write(RGB::new(255, 0, 255)).unwrap();

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
