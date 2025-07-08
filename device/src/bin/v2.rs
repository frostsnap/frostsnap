// Frostsnap custom PCB rev 2.x

#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;
use alloc::string::String;
use core::borrow::BorrowMut;
use cst816s::{TouchGesture, CST816S};
use display_interface_spi::SPIInterface;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_hal as hal;
use esp_hal::{
    delay::Delay, gpio::{Input, Level, Output, Pull}, hmac::Hmac, i2c::master::{Config as i2cConfig, I2c}, ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    }, peripherals::Peripherals, prelude::*, rng::Trng, rsa::Rsa, spi::{
        master::{Config as spiConfig, Spi},
        SpiMode,
    }, timer::{
        self,
        timg::{Timer, TimerGroup},
    }, uart::{self, Uart}, usb_serial_jtag::UsbSerialJtag, Blocking
};
use frostsnap_comms::Downstream;
use frostsnap_core::{schnorr_fun::fun::hex, SignTask};
use frostsnap_device::{
    efuse::{self, EfuseHmacKeys},
    esp32_run,
    graphics::{
        self,
        animation::AnimationProgress,
        widgets::{EnterShareIndexScreen, EnterShareScreen},
    },
    io::SerialInterface,
    ui::{
        BusyTask, EnteringBackupStage, FirmwareUpgradeStatus, Prompt, UiEvent, UserInteraction,
        WaitingFor, WaitingResponse, Workflow,
    },
    DownstreamConnectionState, Instant, UpstreamConnectionState,
};
use micromath::F32Ext;
use mipidsi::{error::Error, models::ST7789, options::ColorInversion};

// # Pin Configuration
//
// GPIO21:     USB UART0 TX  (connect upstream)
// GPIO20:     USB UART0 RX  (connect upstream)
//
// GPIO18:     JTAG/UART1 TX (connect downstream)
// GPIO19:     JTAG/UART1 RX (connect downstream)
//
// GPIO0: Upstream detection
// GPIO10: Downstream detection

macro_rules! init_display {
    (peripherals: $peripherals:ident, delay: $delay:expr) => {{
        let spi = Spi::new_with_config(
            $peripherals.SPI2,
            spiConfig {
                frequency: 80.MHz(),
                mode: SpiMode::Mode2,
                ..spiConfig::default()
            },
        )
        .with_sck($peripherals.GPIO8)
        .with_mosi($peripherals.GPIO7);

        let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
        let di = SPIInterface::new(spi_device, Output::new($peripherals.GPIO9, Level::Low));

        let display = mipidsi::Builder::new(ST7789, di)
            .display_size(240, 280)
            .display_offset(0, 20) // 240*280 panel
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(Output::new($peripherals.GPIO6, Level::Low))
            .init($delay)
            .unwrap();

        display
    }};
}

#[entry]
fn main() -> ! {
    esp_alloc::heap_allocator!(256 * 1024);
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::max();
        config
    });
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let timer0 = timg0.timer0;
    let timer1 = timg1.timer0;

    let mut delay = Delay::new();

    let upstream_detect = Input::new(peripherals.GPIO0, Pull::Up);
    let downstream_detect = Input::new(peripherals.GPIO10, Pull::Up);

    // Turn off backlight to hide artifacts as display initializes
    let mut ledc = Ledc::new(peripherals.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut lstimer0 = ledc.timer::<LowSpeed>(timerledc::Number::Timer0);
    lstimer0
        .configure(timerledc::config::Config {
            duty: timerledc::config::Duty::Duty10Bit,
            clock_source: LSClockSource::APBClk,
            frequency: 24u32.kHz(),
        })
        .unwrap();
    let mut channel0 = ledc.channel(channel::Number::Channel0, peripherals.GPIO1);
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 0, // Turn off backlight to hide artifacts as display initializes
            pin_config: channel::config::PinConfig::PushPull,
        })
        .unwrap();

    let display = init_display!(peripherals: peripherals, delay: &mut delay);
    let mut display = graphics::Graphics::new(display);

    let i2c = I2c::new(
        peripherals.I2C0,
        i2cConfig {
            frequency: 400u32.kHz(),
            ..i2cConfig::default()
        },
    )
    .with_sda(peripherals.GPIO4)
    .with_scl(peripherals.GPIO5);
    let mut capsense = CST816S::new(
        i2c,
        Input::new(peripherals.GPIO2, Pull::Down),
        Output::new(peripherals.GPIO3, Level::Low),
    );
    capsense.setup(&mut delay).unwrap();

    display.clear();
    display.header("Frostsnap");
    display.flush();
    channel0.start_duty_fade(0, 30, 500).unwrap();

    let detect_device_upstream = upstream_detect.is_low();
    let upstream_serial = if detect_device_upstream {
        let serial_conf = uart::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        SerialInterface::new_uart(
            Uart::new_with_config(
                peripherals.UART1,
                serial_conf,
                peripherals.GPIO18,
                peripherals.GPIO19,
            )
            .unwrap(),
            &timer0,
        )
    } else {
        SerialInterface::new_jtag(UsbSerialJtag::new(peripherals.USB_DEVICE), &timer0)
    };
    let downstream_serial: SerialInterface<_, Downstream> = {
        let serial_conf = uart::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };
        let uart = Uart::new_with_config(
            peripherals.UART0,
            serial_conf,
            peripherals.GPIO21,
            peripherals.GPIO20,
        )
        .unwrap();
        SerialInterface::new_uart(uart, &timer0)
    };
    let mut sha256 = esp_hal::sha::Sha::new(peripherals.SHA);
    let rsa = Rsa::new(peripherals.RSA);

    let mut adc = peripherals.ADC1;
    let mut hal_rng = Trng::new(peripherals.RNG, &mut adc);
    // extract more entropy from the trng that we theoretically need
    let mut first_rng = frostsnap_device::extract_entropy(&mut hal_rng, &mut sha256, 1024);

    let efuse = efuse::EfuseController::new(peripherals.EFUSE);

    let do_read_protect = cfg!(feature = "read_protect_hmac_key");

    let hal_hmac = core::cell::RefCell::new(Hmac::new(peripherals.HMAC));
    let mut hmac_keys =
        EfuseHmacKeys::load_or_init(&efuse, &hal_hmac, do_read_protect, &mut hal_rng)
            .expect("should load efuse hmac keys");

    // Don't use the hal_rng directly -- first mix in entropy from the HMAC efuse.
    // TODO: maybe re-key the rng based on entropy from touces etc
    let rng = hmac_keys.fixed_entropy.mix_in_rng(&mut first_rng);

    let ui = FrostyUi {
        display,
        capsense,
        downstream_connection_state: DownstreamConnectionState::Disconnected,
        upstream_connection_state: None,
        workflow: Default::default(),
        device_name: Default::default(),
        changes: false,
        last_touch: None,
        timer: &timer1,
        busy_task: Default::default(),
        recovery_mode: false,
    };

    let run = esp32_run::Run {
        upstream_serial,
        downstream_serial,
        rng,
        ui,
        timer: &timer0,
        downstream_detect,
        sha256,
        rsa,
        hmac_keys,
    };
    run.run()
}

/// Dummy CS pin for our display
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
    display: graphics::Graphics<DT>,
    capsense: CST816S<I2C, PINT, RST>,
    last_touch: Option<(Point, Instant)>,
    downstream_connection_state: DownstreamConnectionState,
    upstream_connection_state: Option<UpstreamConnectionState>,
    workflow: Workflow,
    device_name: Option<String>,
    changes: bool,
    timer: &'t Timer<T, Blocking>,
    busy_task: Option<BusyTask>,
    recovery_mode: bool,
}

impl<T, DT, I2C, PINT, RST, CommE, PinE> FrostyUi<'_, T, DT, I2C, PINT, RST>
where
    T: timer::timg::Instance,
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
    I2C: hal::i2c::I2c<Error = CommE>,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin<Error = PinE>,
{
    fn render(&mut self) {
        if !matches!(self.workflow, Workflow::EnteringBackup { .. }) {
            self.display.clear();
        }
        self.display
            .header(self.device_name.as_deref().unwrap_or("New Device2"));

        match self.workflow.borrow_mut() {
            Workflow::None => {
                if let Some(busy_task) = &self.busy_task {
                    match busy_task {
                        BusyTask::KeyGen => self.display.print("Generating key.."),
                        BusyTask::Signing => self.display.print("Signing.."),
                        BusyTask::VerifyingShare => self.display.print("Verifying key.."),
                        BusyTask::Loading => self.display.print("loading.."),
                        BusyTask::GeneratingNonces => self.display.print("Generating nonces..."),
                    }
                }
            }
            Workflow::FirmwareUpgrade(status) => match status {
                FirmwareUpgradeStatus::Passive => self.display.print("FORWARD MODE"),
                FirmwareUpgradeStatus::Erase { progress } => {
                    self.display.print(if *progress == 1.0 {
                        "Ready to upgrade"
                    } else {
                        "Preparing upgrade.."
                    });
                    self.display.progress_bar(*progress);
                }
                FirmwareUpgradeStatus::Download { progress } => {
                    self.display.print("Downloading firmware..");
                    self.display.progress_bar(*progress);
                }
            },
            Workflow::NamingDevice {
                old_name: _,
                new_name,
            } => {
                self.display.ready_screen(new_name, self.recovery_mode);
            }
            Workflow::WaitingFor(waiting_for) => match waiting_for {
                WaitingFor::WaitingForKeyGenFinalize {
                    key_name,
                    t_of_n,
                    session_hash,
                } => {
                    self.display.show_keygen_pending_finalize(
                        &*key_name,
                        *t_of_n,
                        &format!(
                            "{} {}",
                            hex::encode(&session_hash.0[0..2]),
                            hex::encode(&session_hash.0[2..4])
                        ),
                    );
                }
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
                            self.display.ready_screen(label, self.recovery_mode);
                        }
                        None => {
                            self.display.new_device();
                        }
                    };
                }
                WaitingFor::CoordinatorResponse(response) => match response {
                    WaitingResponse::KeyGen => {
                        self.display.print("Finished!\nWaiting for coordinator..");
                    }
                },
            },
            Workflow::UserPrompt {
                prompt, animation, ..
            } => {
                match prompt {
                    Prompt::Signing { phase } => match &phase.sign_task().inner {
                        SignTask::Test { message } => {
                            self.display.print(format!("Sign: {message}"))
                        }
                        SignTask::Nostr { event } => {
                            self.display.print(format!("Sign nostr: {}", event.content))
                        }
                        SignTask::BitcoinTransaction {
                            tx_template,
                            network,
                        } => {
                            use core::fmt::Write;
                            let mut string = String::new();
                            let prompt_data = tx_template.user_prompt(*network);
                            let foreign_recipients = prompt_data.foreign_recipients;
                            let fee = prompt_data.fee;
                            if foreign_recipients.is_empty() {
                                write!(&mut string, "internal transfer").unwrap();
                            } else {
                                for (address, value) in foreign_recipients {
                                    writeln!(&mut string, "send {value} to {address}").unwrap();
                                }
                            }
                            write!(&mut string, "fee: {fee}").unwrap();
                            self.display.print(string);
                        }
                    },
                    Prompt::KeyGen { phase } => {
                        let session_hash = phase.session_hash();
                        self.display.show_keygen_check(
                            phase.key_name(),
                            phase.t_of_n(),
                            &format!(
                                "{} {}",
                                hex::encode(&session_hash.0[0..2]),
                                hex::encode(&session_hash.0[2..4])
                            ),
                        );
                    }
                    Prompt::NewName { old_name, new_name } => match old_name {
                        Some(old_name) => self.display.print(format!(
                            "Rename this device from '{old_name}' to '{new_name}'?",
                        )),
                        None => self.display.print(format!("Confirm name '{new_name}'?")),
                    },
                    Prompt::DisplayBackupRequest { phase } => self
                        .display
                        .print(format!("Display the backup for key '{}'?", phase.key_name)),
                    Prompt::ConfirmFirmwareUpgrade {
                        firmware_digest,
                        size,
                    } => self.display.print(format!(
                        "Confirm firmware upgrade to: \n{firmware_digest}\nsize: {:.2}KB",
                        *size as f32 / 1000.0
                    )),
                    Prompt::ConfirmEnterBackup { share_backup, .. } => self
                        .display
                        .show_backup(share_backup.to_bech32_backup(), false),
                    Prompt::WipeDevice => self.display.wipe_data_warning(),
                }
                if let Some(completion) = animation.completion() {
                    self.display.confirm_bar(completion);
                }
                self.display.button();
            }
            Workflow::Debug(string) => {
                self.display
                    .print(format!("{}: {}", self.timer.now(), string));
            }
            Workflow::DisplayBackup {
                backup,
                key_name: _,
            } => self.display.show_backup(backup.clone(), false),
            Workflow::EnteringBackup(..) => {
                // this is drawn during poll
            }
            Workflow::DisplayAddress {
                address,
                rand_seed,
                bip32_path,
            } => self.display.show_address(address, bip32_path, *rand_seed),
        }

        if let Some(upstream_connection) = self.upstream_connection_state {
            self.display.upstream_state(upstream_connection);
        }

        self.display
            .downstream_state(self.downstream_connection_state);

        #[cfg(feature = "mem_debug")]
        self.display
            .set_mem_debug(esp_alloc::HEAP.used(), esp_alloc::HEAP.free());

        self.display.flush();
    }
}

impl<T, DT, I2C, PINT, RST, CommE, PinE> UserInteraction for FrostyUi<'_, T, DT, I2C, PINT, RST>
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
        if Some(state) != self.upstream_connection_state {
            self.changes = true;
            self.upstream_connection_state = Some(state);
        }
    }

    fn set_device_name(&mut self, name: Option<impl Into<String>>) {
        let name: Option<String> = name.map(Into::into);
        if name != self.device_name {
            self.device_name = name;
            self.changes = true;
        }
    }

    fn get_device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    fn take_workflow(&mut self) -> Workflow {
        core::mem::take(&mut self.workflow)
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        if matches!(self.workflow, Workflow::Debug(_))
            && !matches!(workflow, Workflow::Debug(_) | Workflow::UserPrompt { .. })
        {
            return;
        }

        self.workflow = workflow;
        self.changes = true;
    }

    fn poll(&mut self) -> Option<UiEvent> {
        // keep the timer register fresh
        let now = self.timer.now();

        let mut event = None;

        let (current_touch, last_touch) = match self.capsense.read_one_touch_event(true) {
            Some(touch) => {
                let corrected_y =
                    touch.y + x_based_adjustment(touch.x) + y_based_adjustment(touch.y);
                let corrected_point = Point::new(touch.x, corrected_y);

                let lift_up = touch.action == 1;
                let last_touch = self.last_touch.take();
                if !lift_up {
                    // lift up is not really a "touch" it's the lack of a touch so it doesn't count here.
                    self.last_touch = Some((corrected_point, now));
                }

                (Some((corrected_point, touch.gesture, lift_up)), last_touch)
            }
            // XXX: We're not interested in last_touch unless there's been a touch because
            // last_touch might be *stuck* because We might miss the "lift_up" event due to screen
            // rendering. So we only make progress on confirm if there is actually a touch right now
            // and we only use last_touch for dragging where these stuck last_touchs don't do any
            // noticible damage.
            None => (None, None),
        };

        let mut workflow = self.take_workflow();
        let mut workflow_finished = false;

        match &mut workflow {
            Workflow::UserPrompt { prompt, animation } => {
                let lift_up = if let Some((_, _, lift_up)) = current_touch {
                    lift_up
                } else {
                    false
                };

                if lift_up {
                    animation.reset();
                    self.changes = true;
                } else if current_touch.is_some() {
                    match animation.poll(now) {
                        AnimationProgress::Progress(progress) => {
                            self.display.confirm_bar(progress);
                        }
                        AnimationProgress::Done => {
                            event = Some(match prompt {
                                Prompt::KeyGen { phase } => UiEvent::KeyGenConfirm {
                                    phase: phase.clone(),
                                },
                                Prompt::Signing { phase } => UiEvent::SigningConfirm {
                                    phase: phase.clone(),
                                },
                                Prompt::NewName { new_name, .. } => {
                                    UiEvent::NameConfirm(new_name.clone())
                                }
                                Prompt::DisplayBackupRequest { phase } => {
                                    UiEvent::BackupRequestConfirm {
                                        phase: phase.clone(),
                                    }
                                }
                                Prompt::ConfirmFirmwareUpgrade { .. } => UiEvent::UpgradeConfirm,
                                Prompt::ConfirmEnterBackup {
                                    phase,
                                    share_backup,
                                } => UiEvent::EnteredShareBackup {
                                    phase: phase.clone(),
                                    share_backup: *share_backup,
                                },
                                Prompt::WipeDevice => UiEvent::WipeDataConfirm,
                            });

                            workflow_finished = true;
                        }
                    }
                }
            }
            Workflow::EnteringBackup(stage) => match stage {
                EnteringBackupStage::Init { phase } => {
                    *stage = EnteringBackupStage::ShareIndex {
                        phase: phase.clone(),
                        screen: EnterShareIndexScreen::new(
                            self.display.display.bounding_box().size,
                        ),
                    };
                }
                EnteringBackupStage::ShareIndex { phase, screen } => {
                    let mut next_screen = None;
                    if let Some((point, _, lift_up)) = current_touch {
                        if let Some(share_index) = screen.handle_touch(point, now, lift_up) {
                            next_screen = Some(EnteringBackupStage::Share {
                                phase: phase.clone(),
                                screen: EnterShareScreen::new(
                                    self.display.display.bounding_box().size,
                                    share_index,
                                ),
                            });
                        }
                    }
                    screen.draw(&mut self.display.display, now);
                    if let Some(next_screen) = next_screen {
                        *stage = next_screen;
                    }
                }
                EnteringBackupStage::Share { phase, screen } => {
                    if let Some((point, gesture, lift_up)) = current_touch {
                        match gesture {
                            TouchGesture::SlideUp | TouchGesture::SlideDown => {
                                screen.handle_vertical_drag(
                                    last_touch.map(|(point, _)| point.y as u32),
                                    point.y as u32,
                                );
                            }
                            _ => {
                                screen.handle_touch(point, now, lift_up);
                                if screen.is_finished() {
                                    match screen.try_create_share() {
                                        Ok(secret_share) => {
                                            self.set_workflow(Workflow::prompt(
                                                Prompt::ConfirmEnterBackup {
                                                    share_backup: secret_share,
                                                    phase: phase.clone(),
                                                },
                                            ));
                                            workflow_finished = true;
                                        }
                                        Err(_e) => {
                                            // for now we just make user keep going until they make it right
                                        }
                                    }
                                }
                            }
                        }
                    }

                    screen.draw(&mut self.display.display, now)
                }
            },
            _ => { /* no user actions to poll */ }
        }

        if !workflow_finished {
            self.workflow = workflow;
        }

        if self.changes {
            self.changes = false;
            self.render();
        }

        event
    }

    fn set_busy_task(&mut self, task: BusyTask) {
        self.changes = Some(task) == self.busy_task;
        self.busy_task = Some(task);
        // HACK: we only display busy task when workflow is None so poll only then to avoid triggering ui events.
        if matches!(self.workflow, Workflow::None) {
            let _event = self.poll().is_none();
            assert!(_event, "no ui events can happen with None workflow");
        }
    }

    fn clear_busy_task(&mut self) {
        self.busy_task = None;
        self.changes = true;
    }

    fn set_recovery_mode(&mut self, value: bool) {
        self.recovery_mode = value;
        self.changes = true;
    }
}

fn x_based_adjustment(x: i32) -> i32 {
    let x = x as f32;
    let corrected = 1.3189e-14 * x.powi(7) - 2.1879e-12 * x.powi(6) - 7.6483e-10 * x.powi(5)
        + 3.2578e-8 * x.powi(4)
        + 6.4233e-5 * x.powi(3)
        - 1.2229e-2 * x.powi(2)
        + 0.8356 * x
        - 20.0;
    (-corrected) as i32
}

fn y_based_adjustment(y: i32) -> i32 {
    if y > 170 {
        return 0;
    }
    let y = y as f32;
    let corrected =
        -5.5439e-07 * y.powi(4) + 1.7576e-04 * y.powi(3) - 1.5104e-02 * y.powi(2) - 2.3443e-02 * y
            + 40.0;
    (-corrected) as i32
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    // XXX: Don't try and remove this steal. This is the only way to get the peripherals after start
    // up.
    let peripherals = unsafe { Peripherals::steal() };
    let mut bl = Output::new(peripherals.GPIO1, Level::Low);

    let mut delay = Delay::new();
    let mut panic_buf = frostsnap_device::panic::PanicBuffer::<512>::default();

    let _ = match info.location() {
        Some(location) => write!(
            &mut panic_buf,
            "{}:{} {}",
            location.file().split('/').next_back().unwrap_or(""),
            location.line(),
            info
        ),
        None => write!(&mut panic_buf, "{info}"),
    };

    let mut display = init_display!(peripherals: peripherals, delay: &mut delay);
    graphics::error_print(&mut display, panic_buf.as_str());
    bl.set_high();

    // switch OTA partition to ota_0/last good one 
    loop {}
}
