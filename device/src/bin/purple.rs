#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

use frostsnap_device::{
    esp32_run::{self, UserInteraction},
    oled,
};

use crate::alloc::string::ToString;
use esp32c3_hal::{
    clock::ClockControl,
    i2c,
    peripherals::Peripherals,
    prelude::*,
    pulse_control::{ClockSource, ConfiguredChannel},
    timer::TimerGroup,
    uart::{config, TxRxPins},
    Delay, PulseControl, Rtc, IO,
};
use esp_backtrace as _;
use esp_hal_smartled::{smartLedAdapter, SmartLedsAdapter};
use smart_leds::{brightness, colors, SmartLedsWrite, RGB};

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

    let delay = Delay::new(&clocks);

    let display = oled::SSD1306::new(
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
    let led = <smartLedAdapter!(1)>::new(pulse.channel0, io.pins.gpio2);

    // Welcome screen
    let upstream_jtag = esp32c3_hal::UsbSerialJtag::new(peripherals.USB_DEVICE);

    let upstream_uart = {
        let serial_conf = config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx1 = TxRxPins::new_tx_rx(
            io.pins.gpio18.into_push_pull_output(),
            io.pins.gpio19.into_floating_input(),
        );
        esp32c3_hal::Uart::new_with_config(
            peripherals.UART1,
            Some(serial_conf),
            Some(txrx1),
            &clocks,
        )
    };

    let downstream_uart = {
        let serial_conf = config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx0 = TxRxPins::new_tx_rx(
            io.pins.gpio21.into_push_pull_output(),
            io.pins.gpio20.into_floating_input(),
        );
        esp32c3_hal::Uart::new_with_config(
            peripherals.UART0,
            Some(serial_conf),
            Some(txrx0),
            &clocks,
        )
    };

    let rng = esp32c3_hal::Rng::new(peripherals.RNG);
    let ui = PurpleUi {
        led,
        display,
        prompt: None,
        delay,
    };
    esp32_run::Run {
        upstream_jtag,
        upstream_uart,
        downstream_uart,
        clocks,
        rng,
        ui,
        timer: timer0,
    }
    .run()
}

struct PurpleUi<'a, C, I> {
    led: SmartLedsAdapter<C, 25>,
    display: oled::SSD1306<'a, I>,
    prompt: Option<PromptState>,
    delay: Delay,
}

#[derive(Clone, Copy, Debug)]
enum PromptState {
    Signing,
    KeyGen,
}

impl<'a, C, I> UserInteraction for PurpleUi<'a, C, I>
where
    C: ConfiguredChannel,
    I: i2c::Instance,
{
    fn splash_screen(&mut self) {
        self.display.print_header("frost snap").unwrap();
        for i in 0..=20 {
            self.led.write([RGB::new(0, i, i)].iter().cloned()).unwrap();
            self.delay.delay_ms(30u32);
        }
        for i in (0..=20).rev() {
            self.led.write([RGB::new(0, i, i)].iter().cloned()).unwrap();
            self.delay.delay_ms(30u32);
        }
    }

    fn waiting_for_upstream(&mut self, looking_at_jtag: bool) {
        self.display
            .print(format!(
                "Waiting for coordinator {}",
                match looking_at_jtag {
                    true => "JTAG",
                    false => "UART",
                }
            ))
            .unwrap();
    }

    fn await_instructions(&mut self, name: &str) {
        self.display.print_header(name).unwrap();
        self.led
            .write(brightness([colors::GREEN].iter().cloned(), 10))
            .unwrap();
    }

    fn confirm_sign(&mut self, sign_task: &frostsnap_core::message::SignTask) {
        self.display.print(format!("Sign {}", sign_task)).unwrap();
        self.prompt = Some(PromptState::Signing);
    }

    fn confirm_key_generated(&mut self, xpub: &str) {
        self.display
            .print(format!("Key generated: {}", xpub))
            .unwrap();
        self.prompt = Some(PromptState::KeyGen);
    }

    fn display_error(&mut self, message: &str) {
        self.display.print(format!("E: {}", message)).unwrap();
    }

    fn poll(&mut self) -> Option<esp32_run::UiEvent> {
        self.prompt.take().map(|prompt| match prompt {
            PromptState::Signing => esp32_run::UiEvent::SigningConfirm(true),
            PromptState::KeyGen => esp32_run::UiEvent::KeyGenConfirm(true),
        })
    }

    fn misc_print(&mut self, string: &str) {
        self.display.print(string).unwrap()
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
