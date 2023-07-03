#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

use frostsnap_device::{
    esp32_run, oled,
    ui::{BusyTask, Prompt, UiEvent, UserInteraction, WaitingFor, WaitingResponse, Workflow},
};

use crate::alloc::string::{String, ToString};
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
use smart_leds::{brightness, colors, SmartLedsWrite};

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
    let timer_group0 = TimerGroup::new(
        peripherals.TIMG0,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = TimerGroup::new(
        peripherals.TIMG1,
        &clocks,
        &mut system.peripheral_clock_control,
    );
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

    let upstream_jtag = esp32c3_hal::UsbSerialJtag::new(
        peripherals.USB_DEVICE,
        &mut system.peripheral_clock_control,
    );

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
            &mut system.peripheral_clock_control,
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
            &mut system.peripheral_clock_control,
        )
    };

    let rng = esp32c3_hal::Rng::new(peripherals.RNG);
    let ui = PurpleUi {
        led,
        display,
        device_label: Default::default(),
        workflow: Workflow::None,
    };

    delay.delay_ms(500u32); // To wait for ESP32c3 timers to stop being bonkers

    esp32_run::Run {
        upstream_jtag,
        upstream_uart,
        downstream_uart,
        rng,
        ui,
        timer: timer0,
    }
    .run()
}

struct PurpleUi<'a, C, I> {
    led: SmartLedsAdapter<C, 25>,
    display: oled::SSD1306<'a, I>,
    workflow: Workflow,
    device_label: Option<String>,
}

impl<'a, C, I> PurpleUi<'a, C, I>
where
    C: ConfiguredChannel,
    I: i2c::Instance,
{
    fn render(&mut self) {
        match &self.workflow {
            Workflow::None => {
                self.led
                    .write(brightness([colors::WHITE].iter().cloned(), 10))
                    .unwrap();
            }
            Workflow::WaitingFor(waiting_for) => match waiting_for {
                WaitingFor::LookingForUpstream { jtag } => {
                    self.led
                        .write(brightness([colors::PURPLE].iter().cloned(), 10))
                        .unwrap();

                    if *jtag {
                        self.display.print("Looking for USB host").unwrap();
                    } else {
                        self.display.print("Looking for upstream device").unwrap();
                    }
                }
                WaitingFor::CoordinatorAnnounceAck => {
                    self.led
                        .write(brightness([colors::PURPLE].iter().cloned(), 10))
                        .unwrap();

                    self.display.print("Waiting app").unwrap();
                }
                WaitingFor::CoordinatorInstruction { completed_task } => {
                    self.led
                        .write(brightness([colors::GREEN].iter().cloned(), 10))
                        .unwrap();

                    let label = self
                        .device_label
                        .as_ref()
                        .expect("label should have been set by now");
                    let mut body = String::new();

                    match completed_task {
                        Some(task) => match task {
                            UiEvent::KeyGenConfirm(ack) => {
                                if *ack {
                                    body.push_str("Key SAVED!\n");
                                }
                            }
                            UiEvent::SigningConfirm(ack) => {
                                if *ack {
                                    body.push_str("SIGNED!\n");
                                }
                            }
                        },
                        None => body.push_str("\n"),
                    };
                    body.push_str(&format!("{}", label));
                    self.display.print_header(label).unwrap();
                    self.display.print(body).unwrap();
                }
                WaitingFor::CoordinatorResponse(response) => match response {
                    WaitingResponse::KeyGen => {
                        self.display.print("Finished keygen!").unwrap();
                    }
                },
            },
            Workflow::UserPrompt(prompt) => {
                self.led
                    .write(brightness([colors::YELLOW].iter().cloned(), 10))
                    .unwrap();

                match prompt {
                    Prompt::Signing(task) => {
                        self.display.print(format!("Sign {}", task)).unwrap();
                    }
                    Prompt::KeyGen(xpub) => {
                        self.display.print(format!("KeyGen {}", xpub)).unwrap();
                    }
                }
            }
            Workflow::BusyDoing(task) => {
                self.led
                    .write(brightness([colors::YELLOW].iter().cloned(), 10))
                    .unwrap();

                match task {
                    BusyTask::KeyGen => self.display.print("Generating key..").unwrap(),
                    BusyTask::Signing => self.display.print("Signing..").unwrap(),
                    BusyTask::VerifyingShare => self.display.print("Verifying key..").unwrap(),
                }
            }
        }
    }
}

impl<'a, C, I> UserInteraction for PurpleUi<'a, C, I>
where
    C: ConfiguredChannel,
    I: i2c::Instance,
{
    fn set_downstream_connection_state(&mut self, _connected: bool) {}

    fn set_device_label(&mut self, label: String) {
        self.device_label = Some(label);
    }

    fn get_device_label(&self) -> Option<&str> {
        self.device_label.as_ref().map(String::as_str)
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        self.workflow = workflow;
        self.render();
    }

    fn display_error(&mut self, message: &str) {
        self.led
            .write(brightness([colors::RED].iter().cloned(), 10))
            .unwrap();

        self.display.print(message).unwrap()
    }

    fn poll(&mut self) -> Option<UiEvent> {
        if let Workflow::UserPrompt(prompt) = &self.workflow {
            return match prompt {
                Prompt::KeyGen(_) => Some(UiEvent::KeyGenConfirm(true)),
                Prompt::Signing(_) => Some(UiEvent::SigningConfirm(true)),
            };
        }

        None
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
