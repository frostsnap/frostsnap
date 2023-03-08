// UART
#![no_std]
#![no_main]

pub mod uart;

extern crate alloc;

use crate::alloc::string::ToString;
use alloc::string::String;
use alloc::vec;
use bincode::{Decode, Encode};
use esp32c3_hal::{
    clock::ClockControl, gpio::IO, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rtc,
    Uart,
};
use esp_backtrace as _;
use esp_hal_common::uart::{config, TxRxPins};
use esp_println::println;
use frostsnap_core::message::CoordinatorToDeviceMessage;
use frostsnap_core::message::CoordinatorToDeviceSend;
use frostsnap_core::message::DeviceSend;
use frostsnap_core::message::DeviceToCoordindatorMessage;
use schnorr_fun::frost;
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

#[derive(Decode, Debug, Clone)]
struct DeviceReceiveSerial {
    #[bincode(with_serde)]
    message: CoordinatorToDeviceMessage,
}

#[derive(Encode, Debug, Clone)]
struct DeviceSendSerial {
    #[bincode(with_serde)]
    message: DeviceToCoordindatorMessage,
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

    let txrx = TxRxPins::new_tx_rx(
        io.pins.gpio21.into_push_pull_output(),
        io.pins.gpio20.into_floating_input(),
    );
    let mut serial = Uart::new_with_config(
        peripherals.UART0,
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
    loop {
        delay.delay_ms(3000 as u32);
        let decoded: Result<DeviceReceiveSerial, _> =
            bincode::decode_from_reader(&mut device_uart, bincode::config::standard());

        let sends = match decoded {
            Ok(message) => {
                let sends = frost_device
                    .recv_coordinator_message(message.message)
                    .unwrap();
                sends
            }
            Err(e) => match frost_device.announce() {
                Some(announce) => {
                    vec![DeviceSend::ToCoordinator(announce)]
                }
                None => {
                    vec![]
                }
            },
        };

        for send in sends {
            match send {
                frostsnap_core::message::DeviceSend::ToCoordinator(msg) => {
                    let serial_msg = DeviceSendSerial { message: msg };
                    if let Err(e) = bincode::encode_into_writer(
                        serial_msg.clone(),
                        &mut device_uart,
                        bincode::config::standard(),
                    ) {
                        // eprintln!("{:?}", e);
                    }
                }
                frostsnap_core::message::DeviceSend::ToUser(_) => todo!(),
            }
        }
    }
    // loop {}
}
