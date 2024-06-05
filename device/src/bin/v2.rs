// Frostsnap custom PCB rev 2.x

#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;
use alloc::string::String;
use core::mem::MaybeUninit;
use cst816s::CST816S;
use display_interface_spi::SPIInterface;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_framebuf::FrameBuf;
use embedded_hal as hal;
use esp_hal::{
    clock::ClockControl,
    delay::Delay,
    gpio::{Input, Io, Level, Output, Pull, NO_PIN},
    i2c::I2C,
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    peripherals::Peripherals,
    prelude::*,
    rng::Rng,
    spi::{master::Spi, SpiMode},
    system::SystemControl,
    timer::{
        self,
        timg::{Timer, TimerGroup},
    },
    uart::{self, Uart},
    usb_serial_jtag::UsbSerialJtag,
    Blocking,
};
use frostsnap_core::schnorr_fun::fun::hex;
use frostsnap_device::{
    esp32_run,
    io::{set_upstream_port_mode_jtag, set_upstream_port_mode_uart, SerialInterface},
    st7789,
    ui::{BusyTask, Prompt, UiEvent, UserInteraction, WaitingFor, WaitingResponse, Workflow},
    DownstreamConnectionState, UpstreamConnectionState,
};
use mipidsi::{error::Error, models::ST7789, options::ColorInversion};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 128 * 1024;
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
///
/// GPIO0: Upstream detection
/// GPIO10: Downstream detection

#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::max(system.clock_control).freeze();

    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks, None);
    let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks, None);
    let timer0 = timer_group0.timer0;
    timer0.start();
    let timer1 = timer_group1.timer0;
    timer1.start();

    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut delay = Delay::new(&clocks);
    // compute instead of using constant for 80MHz cpu speed
    let ticks_per_ms = clocks.cpu_clock.raw() / timer1.divider() / 1_000;

    let upstream_detect = Input::new(io.pins.gpio0, Pull::None);
    let downstream_detect = Input::new(io.pins.gpio10, Pull::None);

    // let bl = Output::new(, Level::Low);
    // Turn off backlight to hide artifacts as display initializes
    let mut ledc = Ledc::new(peripherals.LEDC, &clocks);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut lstimer0 = ledc.get_timer::<LowSpeed>(timerledc::Number::Timer0);
    lstimer0
        .configure(timerledc::config::Config {
            duty: timerledc::config::Duty::Duty10Bit,
            clock_source: LSClockSource::APBClk,
            frequency: 24u32.kHz(),
        })
        .unwrap();
    let mut channel0 = ledc.get_channel(channel::Number::Channel0, io.pins.gpio1);
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 0,
            pin_config: channel::config::PinConfig::PushPull,
        })
        .unwrap();

    let spi = Spi::new(peripherals.SPI2, 40u32.MHz(), SpiMode::Mode2, &clocks)
        .with_sck(io.pins.gpio8)
        .with_mosi(io.pins.gpio7);
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
    let di = SPIInterface::new(spi_device, Output::new(io.pins.gpio9, Level::Low));
    let display = mipidsi::Builder::new(ST7789, di)
        .display_size(240, 280)
        .display_offset(0, 20) // 240*280 panel
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(io.pins.gpio6, Level::Low))
        .init(&mut delay)
        .unwrap();
    let mut framearray = [Rgb565::BLACK; 240 * 280];
    let framebuf = FrameBuf::new(&mut framearray, 240, 280);
    let mut display = st7789::Graphics::new(display, framebuf).unwrap();

    let i2c = I2C::new(
        peripherals.I2C0,
        io.pins.gpio4,
        io.pins.gpio5,
        400u32.kHz(),
        &clocks,
        None,
    );
    let mut capsense = CST816S::new(
        i2c,
        Input::new(io.pins.gpio2, Pull::Down),
        Output::new(io.pins.gpio3, Level::Low),
    );
    capsense.setup(&mut delay).unwrap();

    display.clear(Rgb565::BLACK);
    display.header("Frostsnap");
    display.flush().unwrap();
    channel0.start_duty_fade(0, 30, 500).unwrap();

    let detect_device_upstream = !upstream_detect.is_high();
    let upstream_serial = if detect_device_upstream {
        set_upstream_port_mode_uart();
        let serial_conf = uart::config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx1 = uart::TxRxPins::new_tx_rx(io.pins.gpio18, io.pins.gpio19);
        SerialInterface::new_uart(
            Uart::new_with_config(peripherals.UART1, serial_conf, Some(txrx1), &clocks, None),
            &timer0,
        )
    } else {
        set_upstream_port_mode_jtag();
        SerialInterface::new_jtag(UsbSerialJtag::new(peripherals.USB_DEVICE, None), &timer0)
    };

    let downstream_uart = {
        let serial_conf = uart::config::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let txrx0 = uart::TxRxPins::new_tx_rx(io.pins.gpio21, io.pins.gpio20);
        Uart::new_with_config(peripherals.UART0, serial_conf, Some(txrx0), &clocks, None)
    };

    let mut hal_rng = Rng::new(peripherals.RNG);
    // delay.delay_ms(600u32); // To wait for ESP32c3 timers to stop being bonkers

    let rng = {
        let mut chacha_seed = [0u8; 32];
        hal_rng.read(&mut chacha_seed);
        ChaCha20Rng::from_seed(chacha_seed)
    };

    let ui = FrostyUi {
        display,
        capsense,
        downstream_connection_state: DownstreamConnectionState::Disconnected,
        upstream_connection_state: UpstreamConnectionState::Disconnected,
        workflow: Default::default(),
        device_name: Default::default(),
        changes: false,
        confirm_state: AnimationState::new(&timer1, 600 * 80_000),
        last_touch: None,
        timer: &timer1,
        ticks_per_ms: ticks_per_ms.into(),
    };

    // let _now1 = timer1.now();
    let run = esp32_run::Run {
        upstream_serial,
        downstream_uart,
        rng,
        ui,
        timer: &timer0,
        downstream_detect,
    };
    run.run()
}

/// Noop impl of OutputPin.
struct NoCs;

impl embedded_hal::digital::OutputPin for NoCs {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl embedded_hal::digital::ErrorType for NoCs {
    type Error = core::convert::Infallible;
}

pub struct FrostyUi<'t, T, DT, I2C, PINT, RST> {
    display: st7789::Graphics<'t, DT>,
    capsense: CST816S<I2C, PINT, RST>,
    last_touch: Option<u64>,
    downstream_connection_state: DownstreamConnectionState,
    upstream_connection_state: UpstreamConnectionState,
    workflow: Workflow,
    device_name: Option<String>,
    changes: bool,
    confirm_state: AnimationState<'t, T>,
    timer: &'t Timer<T, Blocking>,
    ticks_per_ms: u64,
}

struct AnimationState<'t, T> {
    timer: &'t Timer<T, Blocking>,
    start: Option<u64>,
    duration_ticks: u64,
    finished: bool,
}

impl<'t, T> AnimationState<'t, T>
where
    T: timer::timg::Instance,
{
    pub fn new(timer: &'t Timer<T, Blocking>, duration_ticks: u64) -> Self {
        Self {
            timer,
            duration_ticks,
            start: None,
            finished: false,
        }
    }

    pub fn reset(&mut self) {
        self.start = None;
        self.finished = false;
    }

    pub fn poll(&mut self) -> AnimationProgress {
        if self.finished {
            return AnimationProgress::Done;
        }
        let now = self.timer.now().ticks();
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

impl<'t, T, DT, I2C, PINT, RST> FrostyUi<'t, T, DT, I2C, PINT, RST>
where
    T: timer::timg::Instance,
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    fn render(&mut self) {
        self.display
            .header(self.device_name.as_deref().unwrap_or("New Device"));

        match &self.workflow {
            Workflow::None => {}
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
                    if *jtag {
                        self.display.print("Looking for coordinator USB host");
                    } else {
                        self.display.print("Looking for upstream device");
                    }
                }
                WaitingFor::CoordinatorAnnounceAck => {
                    self.display.print("Waiting for FrostSnap app");
                }
                WaitingFor::CoordinatorInstruction { completed_task: _ } => {
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
                    Prompt::DisplayBackupRequest(key_id) => self
                        .display
                        .print(format!("Display the backup for key {key_id}?")),
                }
                self.display.button();
            }
            Workflow::BusyDoing(task) => match task {
                BusyTask::KeyGen => self.display.print("Generating key.."),
                BusyTask::Signing => self.display.print("Signing.."),
                BusyTask::VerifyingShare => self.display.print("Verifying key.."),
                BusyTask::Loading => self.display.print("loading.."),
            },
            Workflow::Debug(string) => {
                self.display.print(string);
            }
            Workflow::DisplayBackup { backup } => self.display.print(format!("Backup: {}", backup)),
        }

        match self.upstream_connection_state {
            UpstreamConnectionState::Disconnected => {
                self.display.upstream_state(Rgb565::RED, false);
            }
            UpstreamConnectionState::Connected { is_device } => {
                self.display.upstream_state(Rgb565::YELLOW, is_device);
            }
            UpstreamConnectionState::Established { is_device } => {
                self.display.upstream_state(Rgb565::GREEN, is_device);
            }
        }

        match self.downstream_connection_state {
            DownstreamConnectionState::Disconnected => {
                self.display.downstream_state(None);
            }
            DownstreamConnectionState::Connected => {
                self.display.downstream_state(Some(Rgb565::YELLOW));
            }
            DownstreamConnectionState::Established => {
                self.display.downstream_state(Some(Rgb565::GREEN));
            }
        }

        #[cfg(feature = "mem_debug")]
        self.display
            .set_mem_debug(ALLOCATOR.used(), ALLOCATOR.free());

        self.display.flush().unwrap();
    }
}

impl<'t, T, DT, I2C, PINT, RST, CommE, PinE> UserInteraction for FrostyUi<'t, T, DT, I2C, PINT, RST>
where
    I2C: hal::i2c::I2c<Error = CommE>,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin<Error = PinE>,
    T: timer::timg::Instance,
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    fn set_downstream_connection_state(
        &mut self,
        state: frostsnap_device::DownstreamConnectionState,
    ) {
        if state != self.downstream_connection_state {
            self.changes = true;
            self.downstream_connection_state = state;
        }
    }

    fn set_upstream_connection_state(&mut self, state: frostsnap_device::UpstreamConnectionState) {
        if state != self.upstream_connection_state {
            self.changes = true;
            self.upstream_connection_state = state;
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
        if matches!(self.workflow, Workflow::Debug(_)) && !matches!(workflow, Workflow::Debug(_)) {
            return;
        }
        self.workflow = workflow;
        self.changes = true;
    }

    fn poll(&mut self) -> Option<UiEvent> {
        // keep the timer register fresh
        let now = self.timer.now().ticks();

        let mut event = None;

        if let Workflow::UserPrompt(prompt) = &self.workflow {
            let is_pressed = match self.capsense.read_one_touch_event(true) {
                None => match self.last_touch {
                    None => false,
                    Some(last_touch) => last_touch > now - 10 * self.ticks_per_ms,
                },
                Some(_touch) => {
                    self.last_touch = Some(now);
                    true
                }
            };

            if is_pressed {
                match self.confirm_state.poll() {
                    AnimationProgress::Progress(progress) => {
                        self.display.confirm_bar(progress);
                    }
                    AnimationProgress::FinalTick => {
                        let ui_event = match prompt {
                            Prompt::KeyGen(_) => UiEvent::KeyGenConfirm,
                            Prompt::Signing(_) => UiEvent::SigningConfirm,
                            Prompt::NewName { new_name, .. } => {
                                UiEvent::NameConfirm(new_name.clone())
                            }
                            Prompt::DisplayBackupRequest(key_id) => {
                                UiEvent::BackupRequestConfirm(*key_id)
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
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    // Disable the RTC and TIMG watchdog timers
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut bl = Output::new(io.pins.gpio1, Level::Low);

    let mut delay = Delay::new(&clocks);
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

    let spi = Spi::new(peripherals.SPI2, 40u32.MHz(), SpiMode::Mode2, &clocks).with_pins(
        Some(io.pins.gpio8),
        Some(io.pins.gpio7),
        NO_PIN,
        NO_PIN,
    );
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);

    let di = SPIInterface::new(spi_device, Output::new(io.pins.gpio9, Level::Low));
    let mut display = mipidsi::Builder::new(ST7789, di)
        .display_size(240, 280)
        .display_offset(0, 20) // 240*280 panel
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(io.pins.gpio6, Level::Low))
        .init(&mut delay)
        .unwrap();
    st7789::error_print(&mut display, panic_buf.as_str());
    bl.set_high();

    loop {}
}
