// Frostsnap custom PCB rev 2.x

#![no_std]
#![no_main]

extern crate alloc;
use alloc::{boxed::Box, string::ToString};
use cst816s::{TouchGesture, CST816S};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_hal as hal;
use esp_hal::{
    delay::Delay,
    gpio::{Input, Level, Output, Pull},
    hmac::Hmac,
    i2c::master::{Config as i2cConfig, I2c},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    prelude::*,
    rng::Trng,
    timer::{
        self,
        timg::{Timer, TimerGroup},
    },
    uart::{self, Uart},
    usb_serial_jtag::UsbSerialJtag,
    Blocking,
};
use frostsnap_comms::Downstream;
use frostsnap_embedded_widgets::palette::PALETTE;
use frostsnap_device::{
    debug_stats::create_debug_stats,
    efuse::{self, EfuseHmacKeys},
    esp32_run, init_display,
    io::SerialInterface,
    root_widget::RootWidget,
    touch_calibration::{x_based_adjustment, y_based_adjustment},
    ui::{BusyTask, FirmwareUpgradeStatus, Prompt, UiEvent, UserInteraction, Workflow},
    widget_tree::WidgetTree,
    DownstreamConnectionState, Instant, UpstreamConnectionState,
};
use frostsnap_embedded_widgets::{
    keygen_check::KeygenCheck, sign_prompt::SignPrompt, Alignment, DeviceNameScreen, DynWidget,
    FirmwareUpgradeConfirm, FirmwareUpgradeProgress, Stack, Standby, Welcome, Widget,
};
use mipidsi::error::Error;

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

    let mut display = init_display!(peripherals: peripherals, delay: &mut delay);

    // Clear the screen to black at startup
    display.clear(PALETTE.background).unwrap();

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

    // Initial display setup will be handled by widget tree
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

    let root_widget = RootWidget::new(WidgetTree::default(), 300, PALETTE.background);

    // Build UI stack with root widget and debug stats overlay (create_debug_stats handles feature flags)
    let mut ui_stack = Stack::builder()
        .push(root_widget)
        .push_aligned(create_debug_stats(), Alignment::TopLeft);

    ui_stack.set_constraints(Size::new(240, 280));

    let ui = FrostyUi {
        display: frostsnap_embedded_widgets::SuperDrawTarget::new(display, PALETTE.background),
        ui_stack,
        capsense,
        downstream_connection_state: DownstreamConnectionState::Disconnected,
        upstream_connection_state: None,
        last_touch: None,
        last_redraw_time: Instant::from_ticks(0),
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
        hmac_keys,
    };
    run.run()
}

pub struct FrostyUi<'t, T, DT, I2C, PINT, RST, SW>
where
    DT: DrawTarget<Color = Rgb565>,
    SW: DynWidget,
{
    display: frostsnap_embedded_widgets::SuperDrawTarget<DT, embedded_graphics::pixelcolor::Rgb565>,
    /// Stack composing root_widget with debug stats overlay
    ui_stack: Stack<(RootWidget, SW)>,
    capsense: CST816S<I2C, PINT, RST>,
    last_touch: Option<(Point, Instant)>,
    last_redraw_time: Instant,
    downstream_connection_state: DownstreamConnectionState,
    upstream_connection_state: Option<UpstreamConnectionState>,
    timer: &'t Timer<T, Blocking>,
    busy_task: Option<BusyTask>,
    recovery_mode: bool,
}

impl<T, DT, I2C, PINT, RST, SW, CommE, PinE> UserInteraction
    for FrostyUi<'_, T, DT, I2C, PINT, RST, SW>
where
    SW: Widget<Color = Rgb565>,
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
            self.downstream_connection_state = state;
            self.ui_stack.force_full_redraw();
        }
    }

    fn set_upstream_connection_state(&mut self, state: frostsnap_device::UpstreamConnectionState) {
        if Some(state) != self.upstream_connection_state {
            self.upstream_connection_state = Some(state);
            self.ui_stack.force_full_redraw();
        }
    }

    fn take_workflow(&mut self) -> Workflow {
        // Since we're not storing workflow anymore, return None
        Workflow::None
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        // Check if we can update the current widget instead of switching
        let current_widget = self.ui_stack.children.0.current_mut();

        match (current_widget, &workflow) {
            // If we're already showing a Welcome screen and need a Welcome screen, just leave it
            (WidgetTree::Welcome(_), Workflow::None) => {
                // Already showing Welcome, no need to change
                return;
            }

            // If we're already showing a Standby screen and get another Standby workflow with same content, leave it
            (WidgetTree::Standby(_), Workflow::Standby { .. }) => {
                // TODO: Could check if name/key_name changed and only update if different
                // For now, always switch to show updated names
                return;
            }

            // If we're already showing DeviceNaming and get another NamingDevice workflow, just update the text
            (
                WidgetTree::DeviceNaming(ref mut device_name_screen),
                Workflow::NamingDevice { ref new_name },
            ) => {
                device_name_screen.set_name(new_name.clone());
                return;
            }

            // If we're already showing FirmwareUpgradeProgress, just update it
            (
                WidgetTree::FirmwareUpgradeProgress {
                    widget,
                    ref mut status,
                },
                Workflow::FirmwareUpgrade(ref status_current),
            ) => {
                match (*status, status_current) {
                    (
                        FirmwareUpgradeStatus::Erase { .. },
                        FirmwareUpgradeStatus::Erase { progress },
                    )
                    | (
                        FirmwareUpgradeStatus::Download { .. },
                        FirmwareUpgradeStatus::Download { progress },
                    ) => {
                        *status = *status_current;
                        widget.update_progress(*progress);
                        return;
                    }
                    _ => { /* we need a new widget */ }
                }
            }

            // If we're showing KeygenCheck and get another KeyGen prompt, we need a new one
            // because the security code would be different
            _ => {} // Different widget types, need to switch
        };

        // Convert workflow to widget tree
        let new_page = match workflow {
            Workflow::None => WidgetTree::Welcome(Welcome::new()),
            Workflow::Standby { name, key_name } => {
                WidgetTree::Standby(Standby::new(key_name, name))
            }
            Workflow::UserPrompt(prompt) => {
                match prompt {
                    Prompt::KeyGen { phase } => {
                        // Extract t_of_n and session_hash from phase
                        let t_of_n = phase.t_of_n();
                        let session_hash = phase.session_hash();
                        // Extract the first 4 bytes as security check code
                        let mut security_check_code = [0u8; 4];
                        security_check_code.copy_from_slice(&session_hash.0[..4]);
                        // Create the KeygenCheck widget with just the display data
                        let widget = KeygenCheck::new(t_of_n, security_check_code);
                        // Store both widget and phase in the WidgetTree
                        WidgetTree::KeygenCheck {
                            widget: Box::new(widget),
                            phase: Some(phase),
                        }
                    }
                    Prompt::Signing { phase } => {
                        // Get the sign task from the phase
                        let sign_task = phase.sign_task();

                        // Check what type of signing task this is
                        match &sign_task.inner {
                            frostsnap_core::SignTask::BitcoinTransaction {
                                tx_template,
                                network,
                            } => {
                                // Get the user prompt from the transaction template
                                let prompt = tx_template.user_prompt(*network);

                                // Create the SignPrompt widget
                                let widget = Box::new(SignPrompt::new(prompt));

                                // Store both widget and phase in the WidgetTree
                                WidgetTree::SignPrompt {
                                    widget,
                                    phase: Some(phase),
                                }
                            }
                            _ => {
                                // TODO: Handle other sign task types (Test, Nostr)
                                // For now, just show welcome
                                unimplemented!();
                            }
                        }
                    }
                    Prompt::ConfirmFirmwareUpgrade {
                        firmware_digest,
                        size,
                    } => {
                        // Create the FirmwareUpgradeConfirm widget
                        let widget = Box::new(FirmwareUpgradeConfirm::new(firmware_digest.0, size));

                        // Store the widget and metadata in the WidgetTree
                        WidgetTree::FirmwareUpgradeConfirm {
                            widget,
                            firmware_hash: firmware_digest.0,
                            firmware_size: size,
                            confirmed: false,
                        }
                    }
                    _ => {
                        unimplemented!()
                    }
                }
            }

            Workflow::NamingDevice { new_name } => {
                let device_name_screen = DeviceNameScreen::new(new_name);
                WidgetTree::DeviceNaming(Box::new(device_name_screen))
            }

            Workflow::DisplayBackup { .. } => {
                // TODO: Create backup display screen
                unimplemented!()
            }

            Workflow::EnteringBackup(_) => {
                // TODO: Create backup entry screens
                unimplemented!()
            }

            Workflow::DisplayAddress { .. } => {
                // TODO: Create address display screen
                unimplemented!()
            }

            Workflow::FirmwareUpgrade(status) => {
                use frostsnap_device::ui::FirmwareUpgradeStatus;

                let widget = Box::new(match status {
                    FirmwareUpgradeStatus::Erase { progress } => {
                        FirmwareUpgradeProgress::erasing(progress)
                    }
                    FirmwareUpgradeStatus::Download { progress } => {
                        FirmwareUpgradeProgress::downloading(progress)
                    }
                    FirmwareUpgradeStatus::Passive => FirmwareUpgradeProgress::passive(),
                });

                WidgetTree::FirmwareUpgradeProgress { widget, status }
            }
        };

        // Switch to the new page with fade transition
        self.ui_stack.children.0.switch_to(new_page);
    }

    fn poll(&mut self) -> Option<UiEvent> {
        // keep the timer register fresh
        let now = self.timer.now();
        let current_time = frostsnap_embedded_widgets::Instant::from_millis(
            now.duration_since_epoch().to_millis(),
        );

        // Handle touch input
        if let Some(touch) = self.capsense.read_one_touch_event(true) {
            let corrected_y = touch.y + x_based_adjustment(touch.x) + y_based_adjustment(touch.y);
            let corrected_point = Point::new(touch.x, corrected_y);
            let is_release = touch.action == 1;

            // Handle vertical drag
            if let (Some((last_point, _)), TouchGesture::SlideUp | TouchGesture::SlideDown) =
                (self.last_touch, touch.gesture)
            {
                self.ui_stack.handle_vertical_drag(
                    Some(last_point.y as u32),
                    corrected_y as u32,
                    is_release,
                );
            }

            // Update last touch
            if !is_release {
                self.last_touch = Some((corrected_point, now));
            } else {
                self.last_touch = None;
            }

            // Handle touch
            let _ = self
                .ui_stack
                .handle_touch(corrected_point, current_time, is_release);
        };

        // Only redraw if at least 10ms has passed since last redraw
        let elapsed_ms = (now - self.last_redraw_time).to_millis();
        if elapsed_ms >= 10 {
            // Draw the widget tree
            // Draw the UI stack (includes debug stats overlay)
            let _ = self.ui_stack.draw(&mut self.display, current_time);

            // Update last redraw time
            self.last_redraw_time = now;
        }

        // Check widget states and generate UI events
        match self.ui_stack.children.0.current_mut() {
            WidgetTree::KeygenCheck {
                widget: keygen_check,
                phase,
            } => {
                // Check if confirmed and we still have the phase
                if keygen_check.is_confirmed() && phase.is_some() {
                    // Take the phase (move it out of the Option)
                    if let Some(phase_data) = phase.take() {
                        return Some(UiEvent::KeyGenConfirm { phase: phase_data });
                    }
                }
            }
            WidgetTree::SignPrompt {
                widget: sign_prompt,
                phase,
            } => {
                // Check if confirmed and we still have the phase
                if sign_prompt.is_confirmed() && phase.is_some() {
                    // Take the phase (move it out of the Option)
                    if let Some(phase_data) = phase.take() {
                        return Some(UiEvent::SigningConfirm { phase: phase_data });
                    }
                }
            }
            WidgetTree::FirmwareUpgradeConfirm {
                widget, confirmed, ..
            } => {
                // Check if the firmware upgrade was confirmed and we haven't sent the event yet
                if widget.is_confirmed() && !*confirmed {
                    *confirmed = true; // Mark as confirmed to prevent duplicate events
                    return Some(UiEvent::UpgradeConfirm);
                }
            }
            _ => {}
        }

        None
    }

    fn set_busy_task(&mut self, task: BusyTask) {
        self.busy_task = Some(task);
        // TODO: Update widget tree based on busy task
        self.ui_stack.force_full_redraw();
    }

    fn clear_busy_task(&mut self) {
        self.busy_task = None;
        self.ui_stack.force_full_redraw();
    }

    fn set_recovery_mode(&mut self, value: bool) {
        self.recovery_mode = value;
        self.ui_stack.force_full_redraw();
    }

    fn debug<S: ToString>(&mut self, _debug: S) {
        // Debug text removed - debug stats widget handles debug display
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
