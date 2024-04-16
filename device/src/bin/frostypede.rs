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
use esp_hal::{
    clock::ClockControl,
    gpio::{GpioPin, Input, PullUp},
    peripherals::Peripherals,
    prelude::*,
    rmt::{Rmt, TxChannel},
    spi,
    timer::{self, Timer, TimerGroup},
    uart::{self, Uart},
    Delay, Rng, UsbSerialJtag, IO,
};
use esp_hal_smartled::{smartLedBuffer, SmartLedsAdapter};
use frostsnap_core::schnorr_fun::fun::hex;
use frostsnap_device::{
    esp32_run,
    io::{set_upstream_port_mode_jtag, set_upstream_port_mode_uart},
    st7735::ST7735,
    ui::{BusyTask, Prompt, UiEvent, UserInteraction, WaitingFor, WaitingResponse, Workflow},
    ConnectionState,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use smart_leds::{brightness, colors, SmartLedsWrite, RGB};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    // const HEAP_SIZE: usize = 165 * 1024;
    const HEAP_SIZE: usize = 166 * 1024;
    // const HEAP_SIZE: usize = 166 * 1024;
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

    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks);
    let mut timer0 = timer_group0.timer0;
    timer0.start(1u64.secs());
    let mut timer1 = timer_group1.timer0;
    timer1.start(1u64.secs());

    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut delay = Delay::new(&clocks);
    // compute instead of using constant for 80MHz cpu speed
    let ticks_per_ms = clocks.cpu_clock.raw() / timer1.divider() / 1000;

    // construct the select button
    let select_button = io.pins.gpio9.into_pull_up_input();
    let downstream_detect = io.pins.gpio13.into_pull_up_input();

    let mut bl = io.pins.gpio1.into_push_pull_output();
    bl.set_high().unwrap();

    let framearray = [Rgb565::WHITE; 160 * 80];
    let framebuf = FrameBuf::new(framearray, 160, 80);
    let mut display = ST7735::new(
        io.pins.gpio6.into_push_pull_output().into(),
        io.pins.gpio10.into_push_pull_output().into(),
        peripherals.SPI2,
        io.pins.gpio2,
        io.pins.gpio3,
        &clocks,
        framebuf,
    )
    .unwrap();

    delay.delay_ms(3_000u32);

    let mut filler = vec![];
    let mut i = 0;
    loop {
        // show something
        display.splash_screen((i as f32 / 100.0 % 1.0));
        delay.delay_ms(3_000u32);
        // WE CRASH HERE WITH 166 BYTE HEAP, WE GO FURTHER IF 165
        filler.push(i);
        delay.delay_ms(3_000u32);
        display.set_mem_debug(ALLOCATOR.used(), ALLOCATOR.free());
        i += 1;
    }
}

pub struct FrostyUi<'t, 'd, C, T, SPI>
where
    SPI: spi::master::Instance,
    C: TxChannel,
{
    select_button: GpioPin<Input<PullUp>, 9>,
    led: SmartLedsAdapter<C, 25>,
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
    T: timer::Instance,
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
    C: TxChannel,
    T: timer::Instance,
{
    fn render(&mut self) {
        let splash_progress = self.splash_state.poll();
        match splash_progress {
            AnimationProgress::Progress(progress) => {
                self.display.splash_screen(progress).unwrap();
                return;
            }
            AnimationProgress::FinalTick => {
                self.display.clear(Rgb565::BLACK);
            }
            AnimationProgress::Done => { /* splash is done no need to anything */ }
        }

        self.display
            .header(self.device_name.as_deref().unwrap_or("New Device"));

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
                    .print(&format!("Renaming {}:\n> {}", existing_name, current_name)),
                None => self.display.print(format!("Naming:\n> {}", current_name)),
            },
            Workflow::WaitingFor(waiting_for) => match waiting_for {
                WaitingFor::LookingForUpstream { jtag } => {
                    self.led
                        .write(brightness([colors::PURPLE].iter().cloned(), 10))
                        .unwrap();

                    if *jtag {
                        self.display.print("Looking for coordinator USB host");
                    } else {
                        self.display.print("Looking for upstream device");
                    }
                }
                WaitingFor::CoordinatorAnnounceAck => {
                    self.led
                        .write(brightness([colors::PURPLE].iter().cloned(), 10))
                        .unwrap();
                    self.display.print("Waiting for FrostSnap app");
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
                            self.display.print(body);
                        }
                        None => {
                            self.display.print("Press 'New Device'");
                        }
                    };
                }
                WaitingFor::CoordinatorResponse(response) => match response {
                    WaitingResponse::KeyGen => {
                        self.display.print("Finished!\nWaiting for coordinator..");
                    }
                },
            },
            Workflow::UserPrompt(prompt) => {
                self.led
                    .write(brightness([colors::YELLOW].iter().cloned(), 10))
                    .unwrap();

                match prompt {
                    Prompt::Signing(task) => {
                        self.display.print(format!("Sign {}", task));
                    }
                    Prompt::KeyGen(session_hash) => {
                        self.display
                            .print(format!("Ok {}", hex::encode(session_hash)));
                    }
                    Prompt::NewName { old_name, new_name } => match old_name {
                        Some(old_name) => self.display.print(format!(
                            "Rename this device from '{}' to '{}'?",
                            old_name, new_name
                        )),
                        None => self.display.print(format!("Confirm name '{}'?", new_name)),
                    },
                }
                self.display.confirm_bar(0.0);
            }
            Workflow::BusyDoing(task) => {
                self.led
                    .write(brightness([colors::YELLOW].iter().cloned(), 10))
                    .unwrap();

                match task {
                    BusyTask::KeyGen => self.display.print("Generating key.."),
                    BusyTask::Signing => self.display.print("Signing.."),
                    BusyTask::VerifyingShare => self.display.print("Verifying key.."),
                    BusyTask::Loading => self.display.print("loading.."),
                }
            }
            Workflow::Debug(string) => {
                self.display.print(string);
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
    C: TxChannel,
    T: timer::Instance,
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
                        self.display.confirm_bar(progress);
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

    if let Ok(rmt) = Rmt::new(peripherals.RMT, 80u32.MHz(), &clocks) {
        let rmt_buffer = smartLedBuffer!(1);
        let mut led = SmartLedsAdapter::new(rmt.channel0, io.pins.gpio0, rmt_buffer, &clocks);
        let _ = led.write(brightness([colors::RED].iter().cloned(), 10));
    }

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
    if let Ok(mut display) = ST7735::new(
        io.pins.gpio6.into_push_pull_output().into(),
        io.pins.gpio10.into_push_pull_output().into(),
        peripherals.SPI2,
        io.pins.gpio2,
        io.pins.gpio3,
        &clocks,
        framebuf,
    ) {
        display.error_print(panic_buf.as_str());
    }

    loop {}
}
