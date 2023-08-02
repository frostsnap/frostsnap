#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

use frostsnap_device::{
    buttons::{self, Buttons},
    esp32_run,
    st7735::{self, ST7735},
    ui::{BusyTask, Prompt, UiEvent, UserInteraction, WaitingFor, WaitingResponse, Workflow},
};

use crate::alloc::string::{String, ToString};
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
    let timer_group0 = timer::TimerGroup::new(
        peripherals.TIMG0,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt0 = timer_group0.wdt;
    let timer_group1 = timer::TimerGroup::new(
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

    // construct the 5-position button on the Air101 LCD board
    // orientation: usb-c port on the right
    // down button shares same pin as D5 LED, which pulls the input down enough to cause problems.
    // remove the LED
    let buttons = buttons::Buttons::new(
        io.pins.gpio4.into_pull_up_input().into(),
        io.pins.gpio8.into_pull_up_input().into(),
        io.pins.gpio13.into_pull_up_input().into(),
        io.pins.gpio9.into_pull_up_input().into(),
        io.pins.gpio5.into_pull_up_input().into(),
    );

    let mut bl = io.pins.gpio11.into_push_pull_output();
    // Turn off backlight to hide artifacts as display initializes
    bl.set_low().unwrap();
    let framearray = [Rgb565::WHITE; 160 * 80];
    let framebuf = FrameBuf::new(framearray, 160, 80);
    let display = st7735::ST7735::new(
        // &mut bl,
        io.pins.gpio6.into_push_pull_output().into(),
        io.pins.gpio10.into_push_pull_output().into(),
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
    delay.delay_ms(600u32); // To wait for ESP32c3 timers to stop being bonkers
    bl.set_high().unwrap();

    let ui = BlueUi {
        buttons,
        display,
        user_confirm: true,
        downstream_connected: false,
        workflow: Default::default(),
        device_label: Default::default(),
        splash_state: SplashState::new(&timer1),
    };

    let _now1 = timer1.now();
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

pub struct BlueUi<'t, 'd, T, SPI>
where
    SPI: spi::Instance,
{
    buttons: Buttons,
    display: ST7735<'d, SPI>,
    downstream_connected: bool,
    workflow: Workflow,
    user_confirm: bool,
    device_label: Option<String>,
    splash_state: SplashState<'t, T>,
}

const SPLASH_SCREEN_DURATION: u64 = 40_000 * 600;

struct SplashState<'t, T> {
    timer: &'t esp32c3_hal::timer::Timer<T>,
    splash_screen_start: Option<u64>,
    finished: bool,
}

impl<'t, T> SplashState<'t, T>
where
    T: esp32c3_hal::timer::Instance,
{
    pub fn new(timer: &'t esp32c3_hal::timer::Timer<T>) -> Self {
        Self {
            timer,
            splash_screen_start: None,
            finished: false,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn poll(&mut self) -> SplashProgress {
        if self.finished {
            return SplashProgress::Done;
        }
        let now = self.timer.now();
        match self.splash_screen_start {
            Some(start) => {
                let duration = now.saturating_sub(start);
                if duration < SPLASH_SCREEN_DURATION {
                    SplashProgress::Progress(duration as f32 / SPLASH_SCREEN_DURATION as f32)
                } else {
                    self.finished = true;
                    SplashProgress::FinalTick
                }
            }
            None => {
                self.splash_screen_start = Some(now);
                self.poll()
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SplashProgress {
    Progress(f32),
    FinalTick,
    Done,
}

impl<'t, 'd, T, SPI> BlueUi<'t, 'd, T, SPI>
where
    SPI: spi::Instance,
    T: esp32c3_hal::timer::Instance,
{
    fn render(&mut self) {
        let splash_progress = self.splash_state.poll();
        match splash_progress {
            SplashProgress::Progress(progress) => {
                self.display.splash_screen(progress).unwrap();
                return;
            }
            SplashProgress::FinalTick => {
                self.display.clear(Rgb565::BLACK).unwrap();
                self.display.header("frostsnap").unwrap();
            }
            SplashProgress::Done => { /* splash is done no need to anything */ }
        }
        match &self.workflow {
            Workflow::None => { /* do nothing */ }
            Workflow::WaitingFor(waiting_for) => match waiting_for {
                WaitingFor::LookingForUpstream { jtag } => {
                    if *jtag {
                        // self.display
                        //     .print("Looking for coordinator USB host")
                        //     .unwrap();
                    } else {
                        // self.display.print("Looking for upstream device").unwrap();
                    }
                }
                WaitingFor::CoordinatorAnnounceAck => {
                    // self.display.print("Waiting for FrostSnap app").unwrap();
                }
                WaitingFor::CoordinatorInstruction { completed_task } => {
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
                    body.push_str(&format!("NAME: {}\n", label));

                    body.push_str("Ready..");
                    // self.display.header(label).unwrap();
                    // self.display.print(body).unwrap();
                }
                WaitingFor::CoordinatorResponse(response) => match response {
                    WaitingResponse::KeyGen => {
                        // self.display
                        // .print("Finished!\nWaiting for coordinator..")
                        // .unwrap();
                    }
                },
            },
            Workflow::UserPrompt(prompt) => match prompt {
                Prompt::Signing(task) => {
                    self.display
                        .confirm_view(format!("Sign {}", task), self.user_confirm)
                        .unwrap();
                }
                Prompt::KeyGen(xpub) => {
                    self.display
                        .confirm_view(format!("Ok {}", xpub), self.user_confirm)
                        .unwrap();
                }
            },
            Workflow::BusyDoing(task) => match task {
                BusyTask::KeyGen => self.display.print("Generating key..").unwrap(),
                BusyTask::Signing => self.display.print("Signing..").unwrap(),
                BusyTask::VerifyingShare => self.display.print("Verifying key..").unwrap(),
            },
            Workflow::OnScreenDebug(onscreen_debug) => {
                self.display.print(onscreen_debug.to_string()).unwrap()
            }
        }

        self.display
            .set_top_left_square(match self.downstream_connected {
                true => Rgb565::GREEN,
                false => Rgb565::RED,
            });
    }
}

impl<'d, 't, T, SPI> UserInteraction for BlueUi<'d, 't, T, SPI>
where
    SPI: spi::Instance,
    T: timer::Instance,
{
    fn set_downstream_connection_state(&mut self, connected: bool) {
        if self.downstream_connected != connected {
            // stops downstream poll error handler from spamming render
            self.downstream_connected = connected;
            self.render();
        }
    }

    fn set_device_label(&mut self, label: String) {
        self.device_label = Some(label);
    }

    fn get_device_label(&self) -> Option<&str> {
        self.device_label.as_ref().map(String::as_str)
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        self.workflow = workflow;
        self.user_confirm = true;
        self.render();
    }

    fn poll(&mut self) -> Option<UiEvent> {
        if !self.splash_state.is_finished() {
            self.render();
            return None;
        }
        if let Workflow::UserPrompt(prompt) = &self.workflow {
            match self.buttons.sample_buttons() {
                ButtonDirection::Center => {
                    let ui_event = match prompt {
                        Prompt::KeyGen(_) => UiEvent::KeyGenConfirm(self.user_confirm),
                        Prompt::Signing(_) => UiEvent::SigningConfirm(self.user_confirm),
                    };

                    return Some(ui_event);
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
        }

        None
    }

    fn misc_print(&mut self, string: &str) {
        self.display.print(string).unwrap()
    }

    fn display_error(&mut self, message: &str) {
        self.display.error_print(message).unwrap();
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

    let framearray = [Rgb565::WHITE; 160 * 80];
    let framebuf = FrameBuf::new(framearray, 160, 80);
    // let mut bl = io.pins.gpio11.into_push_pull_output();
    if let Ok(mut display) = st7735::ST7735::new(
        // &mut bl,
        io.pins.gpio6.into_push_pull_output().into(),
        io.pins.gpio10.into_push_pull_output().into(),
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
