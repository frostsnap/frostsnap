// UART
#![no_std]
#![no_main]

pub mod uart;

extern crate alloc;

use crate::alloc::string::ToString;
use alloc::string::String;
use esp32c3_hal::{
    clock::ClockControl, gpio::IO, peripherals::Peripherals, prelude::*, timer::TimerGroup, Rtc,
    Uart,
};
use esp_backtrace as _;
use esp_hal_common::uart::{config, TxRxPins};
// use esp_println::println;
use frostsnap_core::message::CoordinatorToDeviceMessage;

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

#[derive(bincode::Decode, Debug, bincode::Encode)]
struct FrostMessage {
    message: String,
}

#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    // default 80MHz
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    // let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock160MHz).freeze();

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
    let mut device_uart = uart::DeviceUart::new(serial);

    let mut prev_time = timer0.now();
    loop {
        // if timer0.now() - prev_time > 10 {
        if true {
            prev_time = timer0.now();
            let decoded: Result<FrostMessage, _> =
                bincode::decode_from_reader(&mut device_uart, bincode::config::standard());

            let message_response = match decoded {
                Ok(message) => {
                    // println!("{:?}", message);
                    "we got your message!".to_string()
                }
                Err(e) => {
                    // println!("{:?}", e);
                    "hello can you hear us".to_string()
                }
            };

            bincode::encode_into_writer(
                FrostMessage {
                    message: message_response,
                },
                &mut device_uart,
                bincode::config::standard(),
            );
        }
    }

    // loop {}
}
