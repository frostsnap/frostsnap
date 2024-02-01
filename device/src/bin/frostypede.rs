// Frostsnap custom PCB rev 1.1
// GPIO13 Downstream detection
// GPIO5 Left button
// GPIO9 Right button
// GPIO0 RGB LED

#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;
use crate::alloc::string::String;
use core::mem::MaybeUninit;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_framebuf::FrameBuf;
use esp_backtrace as _;
use esp_hal_smartled::{smartLedAdapter, SmartLedsAdapter};
use frostsnap_core::schnorr_fun::fun::hex;
use frostsnap_device::{
    esp32_run,
    io::{set_upstream_port_mode_jtag, set_upstream_port_mode_uart},
    st7735::{self, ST7735},
    ui::{BusyTask, Prompt, UiEvent, UserInteraction, WaitingFor, WaitingResponse, Workflow},
    ConnectionState,
};
use hal::{
    clock::ClockControl,
    gpio::{GpioPin, Input, PullUp},
    peripherals::Peripherals,
    prelude::*,
    rmt::{Rmt, TxChannel},
    spi,
    timer::{Timer, TimerGroup},
    uart::{self, Uart},
    Delay, Rtc, UsbSerialJtag, IO,
};
use smart_leds::{brightness, colors, SmartLedsWrite, RGB};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 300 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
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
    // First thing we do otherwise it appears as JTAG to the OS first and then it switches. If this
    // is annoying maybe we can make a feature flag to do it later because it seems that espflash
    // relies on the device being in jtag mode immediately after reseting it.
    set_upstream_port_mode_uart();

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
    timer0.start(1u64.secs());
    let mut timer1 = timer_group1.timer0;
    timer1.start(1u64.secs());

    rtc.swd.disable();
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut delay = Delay::new(&clocks);
    // compute instead of using constant for 80MHz cpu speed
    let ticks_per_ms = clocks.cpu_clock.raw() / timer1.divider() / 1000;

    // construct the select button
    let select_button = io.pins.gpio9.into_pull_up_input();
    let downstream_detect = io.pins.gpio13.into_pull_up_input();

    let mut bl = io.pins.gpio1.into_push_pull_output();
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
        &clocks,
        framebuf,
    )
    .unwrap();

    // RGB LED
    let rmt = Rmt::new(peripherals.RMT, 80u32.MHz(), &clocks).unwrap();
    let mut led = <smartLedAdapter!(0, 1)>::new(rmt.channel0, io.pins.gpio0);
    led.write(brightness([colors::BLACK].iter().cloned(), 0))
        .unwrap();

    let upstream_jtag = UsbSerialJtag::new(peripherals.USB_DEVICE);

    let upstream_uart = {
        let serial_conf = uart::config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx1 = uart::TxRxPins::new_tx_rx(
            io.pins.gpio18.into_push_pull_output(),
            io.pins.gpio19.into_floating_input(),
        );
        hal::Uart::new_with_config(peripherals.UART1, serial_conf, Some(txrx1), &clocks)
    };

    let downstream_uart = {
        let serial_conf = uart::config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx0 = uart::TxRxPins::new_tx_rx(
            io.pins.gpio21.into_push_pull_output(),
            io.pins.gpio20.into_floating_input(),
        );
        Uart::new_with_config(peripherals.UART0, serial_conf, Some(txrx0), &clocks)
    };

    let rng = hal::Rng::new(peripherals.RNG);
    delay.delay_ms(600u32); // To wait for ESP32c3 timers to stop being bonkers
    bl.set_high().unwrap();

    let ui = FrostyUi {
        select_button,
        led,
        display,
        downstream_connection_state: ConnectionState::Disconnected,
        workflow: Default::default(),
        device_name: Default::default(),
        splash_state: AnimationState::new(&timer1, (600 * ticks_per_ms).into()),
        changes: false,
        confirm_state: AnimationState::new(&timer1, (700 * ticks_per_ms).into()),
        timer: &timer1,
    };

    // let _now1 = timer1.now();
    esp32_run::Run {
        upstream_jtag,
        upstream_uart,
        downstream_uart,
        rng,
        ui,
        timer: timer0,
        downstream_detect,
    }
    .run()
}

pub struct FrostyUi<'t, 'd, C, T, SPI>
where
    SPI: spi::master::Instance,
    C: TxChannel<0>,
{
    select_button: GpioPin<Input<PullUp>, 9>,
    led: SmartLedsAdapter<C, 0, 25>,
    display: ST7735<'d, SPI>,
    downstream_connection_state: ConnectionState,
    workflow: Workflow,
    device_name: Option<String>,
    splash_state: AnimationState<'t, T>,
    changes: bool,
    confirm_state: AnimationState<'t, T>,
    timer: &'t Timer<T>,
}

struct AnimationState<'t, T> {
    timer: &'t Timer<T>,
    start: Option<u64>,
    duration_ticks: u64,
    finished: bool,
}

impl<'t, T> AnimationState<'t, T>
where
    T: hal::timer::Instance,
{
    pub fn new(timer: &'t Timer<T>, duration_ticks: u64) -> Self {
        Self {
            timer,
            duration_ticks,
            start: None,
            finished: false,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn reset(&mut self) {
        self.start = None;
        self.finished = false;
    }

    pub fn poll(&mut self) -> AnimationProgress {
        if self.finished {
            return AnimationProgress::Done;
        }
        let now = self.timer.now();
        match self.start {
            Some(start) => {
                let duration = now.saturating_sub(start);
                if duration < self.duration_ticks {
                    AnimationProgress::Progress(duration as f32 / self.duration_ticks as f32)
                } else {
                    self.finished = true;
                    AnimationProgress::FinalTick
                }
            }
            None => {
                self.start = Some(now);
                self.poll()
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AnimationProgress {
    Progress(f32),
    FinalTick,
    Done,
}

impl<'t, 'd, C, T, SPI> FrostyUi<'t, 'd, C, T, SPI>
where
    SPI: spi::master::Instance,
    C: TxChannel<0>,
    T: hal::timer::Instance,
{
    fn render(&mut self) {
        let splash_progress = self.splash_state.poll();
        match splash_progress {
            AnimationProgress::Progress(progress) => {
                self.display.splash_screen(progress).unwrap();
                return;
            }
            AnimationProgress::FinalTick => {
                self.display.clear(Rgb565::BLACK).unwrap();
            }
            AnimationProgress::Done => { /* splash is done no need to anything */ }
        }

        self.display
            .header(self.device_name.as_deref().unwrap_or("New Device"))
            .unwrap();

        match &self.workflow {
            Workflow::None => {
                self.led
                    .write(brightness([colors::WHITE].iter().cloned(), 10))
                    .unwrap();
            }
            Workflow::NamingDevice {
                old_name: existing_name,
                new_name: current_name,
            } => match existing_name {
                Some(existing_name) => self
                    .display
                    .print(&format!("Renaming {}:\n> {}", existing_name, current_name))
                    .unwrap(),
                None => self
                    .display
                    .print(format!("Naming:\n> {}", current_name))
                    .unwrap(),
            },
            Workflow::WaitingFor(waiting_for) => match waiting_for {
                WaitingFor::LookingForUpstream { jtag } => {
                    self.led
                        .write(brightness([colors::PURPLE].iter().cloned(), 10))
                        .unwrap();

                    if *jtag {
                        self.display
                            .print("Looking for coordinator USB host")
                            .unwrap();
                    } else {
                        self.display.print("Looking for upstream device").unwrap();
                    }
                }
                WaitingFor::CoordinatorAnnounceAck => {
                    self.led
                        .write(brightness([colors::PURPLE].iter().cloned(), 10))
                        .unwrap();
                    self.display.print("Waiting for FrostSnap app").unwrap();
                }
                WaitingFor::CoordinatorInstruction { completed_task: _ } => {
                    self.led
                        .write(brightness([colors::GREEN].iter().cloned(), 10))
                        .unwrap();

                    match &self.device_name {
                        Some(label) => {
                            let mut body = String::new();
                            body.push_str(&format!("NAME: {}\n", label));

                            body.push_str("Ready..");
                            self.display.print(body).unwrap();
                        }
                        None => {
                            self.display.print("Press 'New Device'").unwrap();
                        }
                    };
                }
                WaitingFor::CoordinatorResponse(response) => match response {
                    WaitingResponse::KeyGen => {
                        self.display
                            .print("Finished!\nWaiting for coordinator..")
                            .unwrap();
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
                    Prompt::KeyGen(session_hash) => {
                        self.display
                            .print(format!("Ok {}", hex::encode(session_hash)))
                            .unwrap();
                    }
                    Prompt::NewName { old_name, new_name } => match old_name {
                        Some(old_name) => self
                            .display
                            .print(format!(
                                "Rename this device from '{}' to '{}'?",
                                old_name, new_name
                            ))
                            .unwrap(),
                        None => self
                            .display
                            .print(format!("Confirm name '{}'?", new_name))
                            .unwrap(),
                    },
                }
                self.display.confirm_bar(0.0).unwrap();
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
            Workflow::Debug(string) => {
                self.display.print(string).unwrap();
            }
        }

        self.display
            .set_top_left_square(match self.downstream_connection_state {
                ConnectionState::Disconnected => Rgb565::RED,
                ConnectionState::Connected => Rgb565::YELLOW,
                ConnectionState::Established => Rgb565::GREEN,
            });

        #[cfg(feature = "mem_debug")]
        self.display
            .set_mem_debug(ALLOCATOR.used(), ALLOCATOR.free());

        self.display.flush().unwrap();
    }
}

impl<'d, 't, C, T, SPI> UserInteraction for FrostyUi<'d, 't, C, T, SPI>
where
    SPI: spi::master::Instance,
    C: TxChannel<0>,
    T: hal::timer::Instance,
{
    fn set_downstream_connection_state(&mut self, state: ConnectionState) {
        if state != self.downstream_connection_state {
            self.changes = true;
            self.downstream_connection_state = state;
        }
    }

    fn set_device_name(&mut self, name: String) {
        self.device_name = Some(name);
        self.changes = true;
    }

    fn get_device_label(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    fn take_workflow(&mut self) -> Workflow {
        core::mem::take(&mut self.workflow)
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        if let Workflow::Debug(_) = self.workflow {
            return;
        }
        self.workflow = workflow;
        self.changes = true;
    }

    fn poll(&mut self) -> Option<UiEvent> {
        // keep the timer register fresh
        let _now = self.timer.now();
        let mut event = None;
        if !self.splash_state.is_finished() {
            self.render();
            return event;
        }

        if let Workflow::UserPrompt(prompt) = &self.workflow {
            if self.select_button.is_low().unwrap() {
                match self.confirm_state.poll() {
                    AnimationProgress::Progress(progress) => {
                        self.led
                            .write(brightness(
                                [RGB::new(0, (128.0 * progress) as u8, 0)].iter().cloned(),
                                30,
                            ))
                            .unwrap();
                        self.display.confirm_bar(progress).unwrap();
                    }
                    AnimationProgress::FinalTick => {
                        self.led
                            .write(brightness([colors::GREEN].iter().cloned(), 30))
                            .unwrap();
                        let ui_event = match prompt {
                            Prompt::KeyGen(_) => UiEvent::KeyGenConfirm,
                            Prompt::Signing(_) => UiEvent::SigningConfirm,
                            Prompt::NewName { new_name, .. } => {
                                UiEvent::NameConfirm(new_name.clone())
                            }
                        };
                        event = Some(ui_event);
                    }
                    AnimationProgress::Done => {}
                }
            } else {
                // deal with button released before confirming
                if self.confirm_state.start.is_some() {
                    self.confirm_state.reset();
                    self.changes = true;
                }
            }
        }

        if self.changes {
            self.changes = false;
            self.render();
        }

        event
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    set_upstream_port_mode_jtag();
    let peripherals = unsafe { Peripherals::steal() };
    let system = peripherals.SYSTEM.split();
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    // Disable the RTC and TIMG watchdog timers
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    // RGB LED
    // White: found coordinator
    // Blue: found another device upstream
    let rmt = Rmt::new(peripherals.RMT, 80u32.MHz(), &clocks).unwrap();
    let mut led = <smartLedAdapter!(0, 1)>::new(rmt.channel0, io.pins.gpio0);
    led.write(brightness([colors::RED].iter().cloned(), 10))
        .unwrap();

    let mut panic_buf = frostsnap_device::panic::PanicBuffer::<512>::default();

    let _ = match info.location() {
        Some(location) => write!(
            &mut panic_buf,
            "{}:{} {}",
            location.file().split('/').last().unwrap_or(""),
            location.line(),
            info
        ),
        None => write!(&mut panic_buf, "{}", info),
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
        &clocks,
        framebuf,
    ) {
        let _ = display.error_print(panic_buf.as_str());
    }

    loop {}
}
