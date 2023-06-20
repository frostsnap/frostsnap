#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

use frostsnap_device::{
    buttons::{self, Buttons},
    esp32_run::{self, UserInteraction},
    st7735::{self, ST7735},
};

use crate::alloc::string::{String, ToString};
use esp32c3_hal::gpio::BankGpioRegisterAccess;
use esp32c3_hal::gpio::InteruptStatusRegisterAccess;
use esp32c3_hal::{
    clock::ClockControl,
    peripherals::Peripherals,
    prelude::*,
    spi, timer,
    uart::{config, TxRxPins},
    Delay, IO,
};
use esp_backtrace as _;

use buttons::ButtonDirection;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_framebuf::FrameBuf;

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
    let mut rtc = esp32c3_hal::Rtc::new(peripherals.RTC_CNTL);
    let timer_group0 = timer::TimerGroup::new(peripherals.TIMG0, &clocks);
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = timer::TimerGroup::new(peripherals.TIMG1, &clocks);
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
    let buttons = buttons::Buttons::new(
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
    let display = st7735::ST7735::new(
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

    let ui = BlueUi {
        buttons,
        display,
        user_confirm: true,
        user_prompt: None,
    };

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
    bl.set_high().unwrap();
    delay.delay_ms(20u32);

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

pub struct BlueUi<'d, RA, IRA, SPI>
where
    RA: BankGpioRegisterAccess,
    IRA: InteruptStatusRegisterAccess,
    SPI: spi::Instance,
{
    buttons: Buttons<RA, IRA>,
    display: ST7735<'d, RA, IRA, SPI>,
    user_confirm: bool,
    user_prompt: Option<PromptState>,
}

#[derive(Clone, Debug)]
enum PromptState {
    KeyGen(String),
    Signing(String),
}

impl<'d, RA, IRA, SPI> BlueUi<'d, RA, IRA, SPI>
where
    RA: BankGpioRegisterAccess,
    IRA: InteruptStatusRegisterAccess,
    SPI: spi::Instance,
{
    fn render(&mut self) {
        match &self.user_prompt {
            Some(PromptState::Signing(task)) => {
                self.display
                    .confirm_view(format!("Sign {}", task), self.user_confirm)
                    .unwrap();
            }
            Some(PromptState::KeyGen(xpub)) => {
                self.display
                    .confirm_view(format!("Ok {}", xpub), self.user_confirm)
                    .unwrap();
            }
            None => {
                /* we should not have an option rather we should just have a view with many states */
            }
        }
    }
}

impl<'d, RA, IRA, SPI> UserInteraction for BlueUi<'d, RA, IRA, SPI>
where
    RA: BankGpioRegisterAccess,
    IRA: InteruptStatusRegisterAccess,
    SPI: spi::Instance,
{
    fn splash_screen(&mut self) {
        self.display.splash_screen().unwrap();
        self.display.clear(Rgb565::BLACK).unwrap();
        self.display.header("frostsnap").unwrap();
        self.display.flush().unwrap();
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
        self.display.header(name).unwrap();
        self.display.flush().unwrap();
    }

    fn confirm_sign(&mut self, sign_task: &frostsnap_core::message::SignTask) {
        self.user_prompt = Some(PromptState::Signing(sign_task.to_string()));
        self.user_confirm = true;
        self.render()
    }

    fn confirm_key_generated(&mut self, xpub: &str) {
        self.user_prompt = Some(PromptState::KeyGen(xpub.into()));
        self.user_confirm = true;
        self.render()
    }

    fn display_error(&mut self, message: &str) {
        self.display.error_print(message).unwrap();
    }

    fn poll(&mut self) -> Option<esp32_run::UiEvent> {
        match self.buttons.sample_buttons() {
            ButtonDirection::Center => {
                if let Some(prompt) = &self.user_prompt {
                    let ui_event = match prompt {
                        PromptState::KeyGen(_) => {
                            esp32_run::UiEvent::KeyGenConfirm(self.user_confirm)
                        }
                        PromptState::Signing(_) => {
                            esp32_run::UiEvent::SigningConfirm(self.user_confirm)
                        }
                    };

                    let print = match ui_event {
                        esp32_run::UiEvent::KeyGenConfirm(true) => "Key accepted",
                        esp32_run::UiEvent::SigningConfirm(true) => "Signing request accepted",
                        esp32_run::UiEvent::KeyGenConfirm(false) => "Key rejected",
                        esp32_run::UiEvent::SigningConfirm(false) => "Signing request rejected",
                    };

                    self.display.print(print).unwrap();
                    self.user_prompt = None;
                    return Some(ui_event);
                }
            }
            ButtonDirection::Right => {
                self.user_confirm = true;
                self.render();
            }
            ButtonDirection::Left => {
                self.user_confirm = false;
                self.render();
            }
            ButtonDirection::Unpressed => {}
            _ => {}
        }

        None
    }

    fn misc_print(&mut self, string: &str) {
        self.display.print(string).unwrap();
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
