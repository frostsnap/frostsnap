// Frostsnap custom PCB rev 2.x

#![no_std]
#![no_main]

extern crate alloc;
use alloc::{boxed::Box, format};
use core::convert::TryInto;
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
use frostsnap_device::{
    efuse::{self, EfuseHmacKeys},
    esp32_run, init_display,
    io::SerialInterface,
    root_widget::RootWidget,
    touch_calibration::adjust_touch_point,
    ui::{BusyTask, FirmwareUpgradeStatus, Prompt, UiEvent, UserInteraction, Workflow},
    widget_tree::WidgetTree,
    DownstreamConnectionState, Instant, UpstreamConnectionState,
};
use frostsnap_widgets::palette::PALETTE;
use frostsnap_widgets::{
    backup::{BackupDisplay, EnterShareScreen},
    debug::{EnabledDebug, OverlayDebug},
    keygen_check::KeygenCheck,
    sign_prompt::SignPrompt,
    DeviceNameScreen, DynWidget, FirmwareUpgradeConfirm, FirmwareUpgradeProgress, Standby, Welcome,
    Widget,
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
    // Capture stack pointer at the very beginning
    frostsnap_widgets::init_log_stack_pointer!();

    esp_alloc::heap_allocator!(240 * 1024);

    // Initialize debug logging early
    frostsnap_widgets::debug::init_logging();
    frostsnap_widgets::debug::log("Debug logging initialized".into());

    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::max();
        config
    });
    #[cfg(feature = "stack_guard")]
    frostsnap_device::stack_guard::enable_stack_guard(peripherals.ASSIST_DEBUG);
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

    let root_widget = RootWidget::new(
        WidgetTree::default(),
        0, /* 0 =  disable fading for now */
        PALETTE.background,
    );

    // Create root widget with debug overlay
    let debug_config = EnabledDebug {
        logs: cfg!(feature = "debug_log"),
        memory: cfg!(feature = "debug_mem"),
        fps: cfg!(feature = "debug_fps"),
    };
    let mut widget_with_debug = OverlayDebug::new(root_widget, debug_config);
    widget_with_debug.set_constraints(Size::new(240, 280));

    let ui = Box::new(FrostyUi {
        display: frostsnap_widgets::SuperDrawTarget::new(display, PALETTE.background),
        widget: widget_with_debug,
        capsense,
        downstream_connection_state: DownstreamConnectionState::Disconnected,
        upstream_connection_state: None,
        last_touch: None,
        last_redraw_time: Instant::from_ticks(0),
        current_widget_index: 0,
        timer: &timer1,
        busy_task: Default::default(),
    });

    let mut run = Box::new(esp32_run::Run {
        upstream_serial,
        downstream_serial,
        rng,
        ui,
        timer: &timer0,
        downstream_detect,
        sha256,
        hmac_keys,
    });
    run.run()
}

pub struct FrostyUi<'t, T, DT, I2C, PINT, RST>
where
    DT: DrawTarget<Color = Rgb565>,
{
    display: frostsnap_widgets::SuperDrawTarget<DT, embedded_graphics::pixelcolor::Rgb565>,
    widget: OverlayDebug<RootWidget>,
    capsense: CST816S<I2C, PINT, RST>,
    last_touch: Option<Point>,
    last_redraw_time: Instant,
    downstream_connection_state: DownstreamConnectionState,
    upstream_connection_state: Option<UpstreamConnectionState>,
    timer: &'t Timer<T, Blocking>,
    busy_task: Option<BusyTask>,
    current_widget_index: usize,
}

impl<T, DT, I2C, PINT, RST, CommE, PinE> UserInteraction for FrostyUi<'_, T, DT, I2C, PINT, RST>
where
    I2C: hal::i2c::I2c<Error = CommE>,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin<Error = PinE>,
    T: timer::timg::Instance,
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    #[inline(never)]
    fn set_downstream_connection_state(
        &mut self,
        state: frostsnap_device::DownstreamConnectionState,
    ) {
        if state != self.downstream_connection_state {
            self.downstream_connection_state = state;
        }
    }

    fn set_upstream_connection_state(&mut self, state: frostsnap_device::UpstreamConnectionState) {
        if Some(state) != self.upstream_connection_state {
            self.upstream_connection_state = Some(state);
        }
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        // Check if we can update the current widget instead of switching
        let current_widget = self.widget.inner_mut().current_mut();

        match (current_widget, &workflow) {
            // If we're already showing a Welcome screen and need a Welcome screen, just leave it
            (WidgetTree::Welcome(_), Workflow::None) => {
                // Already showing Welcome, no need to change
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
            Workflow::None => WidgetTree::Welcome(Box::new(Welcome::new())),
            Workflow::Standby {
                device_name,
                held_share,
            } => WidgetTree::Standby(Box::new(Standby::new(device_name, held_share))),
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
                    Prompt::DisplayBackupRequest { phase } => {
                        use frostsnap_widgets::{HoldToConfirm, Text, FONT_MED};
                        use u8g2_fonts::U8g2TextStyle;

                        // Create text for the prompt
                        let key_name = &phase.key_name;
                        let prompt_text = Text::new(
                            format!("Display backup\nfor\n{}", key_name),
                            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
                        )
                        .with_alignment(embedded_graphics::text::Alignment::Center);

                        // Create HoldToConfirm widget with 2 second hold time
                        let hold_to_confirm = HoldToConfirm::new(2000, prompt_text);

                        // Store in WidgetTree - we need to add a new variant for this
                        WidgetTree::DisplayBackupRequestPrompt {
                            widget: Box::new(hold_to_confirm),
                            phase: Some(phase),
                        }
                    }
                    Prompt::NewName { old_name, new_name } => {
                        use frostsnap_widgets::{HoldToConfirm, Text, FONT_MED};
                        use u8g2_fonts::U8g2TextStyle;

                        // Create text for the prompt
                        let prompt_text = if let Some(old_name) = old_name {
                            format!("Rename device\nfrom '{}'\nto '{}'?", old_name, new_name)
                        } else {
                            format!("Name device\n'{}'?", new_name)
                        };

                        let text_widget = Text::new(
                            prompt_text,
                            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
                        )
                        .with_alignment(embedded_graphics::text::Alignment::Center);

                        // Create HoldToConfirm widget with 2 second hold time
                        let hold_to_confirm = HoldToConfirm::new(2000, text_widget);

                        WidgetTree::NewNamePrompt {
                            widget: Box::new(hold_to_confirm),
                            new_name: Some(new_name.clone()),
                        }
                    }
                    Prompt::WipeDevice => {
                        use frostsnap_widgets::{HoldToConfirm, Text, FONT_MED};
                        use u8g2_fonts::U8g2TextStyle;

                        // Create warning text for device wipe
                        let prompt_text = "WARNING!\n\nErase all data?\n\nHold to confirm";

                        let text_widget =
                            Text::new(prompt_text, U8g2TextStyle::new(FONT_MED, PALETTE.error))
                                .with_alignment(embedded_graphics::text::Alignment::Center);

                        // Create HoldToConfirm widget with 3 second hold time for wipe
                        let hold_to_confirm = HoldToConfirm::new(3000, text_widget);

                        WidgetTree::WipeDevicePrompt {
                            widget: Box::new(hold_to_confirm),
                            confirmed: false,
                        }
                    }
                }
            }

            Workflow::NamingDevice { new_name } => {
                let device_name_screen = DeviceNameScreen::new(new_name);
                WidgetTree::DeviceNaming(Box::new(device_name_screen))
            }

            Workflow::DisplayBackup {
                key_name: _,
                backup,
            } => {
                let word_indices = backup.to_word_indices();
                let share_index: u16 = backup
                    .index()
                    .try_into()
                    .expect("Share index should fit in u16");
                let backup_display = BackupDisplay::new(word_indices, share_index);
                WidgetTree::DisplayBackup(Box::new(backup_display))
            }

            Workflow::EnteringBackup(phase) => {
                let mut widget = Box::new(EnterShareScreen::new());
                if cfg!(feature = "prefill-words") {
                    widget.prefill_test_words();
                }
                WidgetTree::EnterBackup {
                    widget,
                    phase: Some(phase),
                }
            }

            Workflow::DisplayAddress {
                address,
                bip32_path,
                ..
            } => {
                use frostsnap_widgets::AddressWithPath;

                // Create the address display widget
                let address_display = AddressWithPath::new(address, bip32_path);
                WidgetTree::AddressDisplay(Box::new(address_display))
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
        self.widget.inner_mut().switch_to(new_page);
    }

    fn poll(&mut self) -> Option<UiEvent> {
        let now = self.timer.now();
        let now_ms =
            frostsnap_widgets::Instant::from_millis(now.duration_since_epoch().to_millis());

        // Handle touch input

        if let Some(touch_event) = self.capsense.read_one_touch_event(true) {
            // Only process if we have valid coordinates
            // Apply touch calibration adjustments
            let touch_point = adjust_touch_point(Point::new(touch_event.x, touch_event.y));
            let lift_up = touch_event.action == 1;
            let gesture = touch_event.gesture;

            let is_vertical_drag =
                matches!(gesture, TouchGesture::SlideUp | TouchGesture::SlideDown);
            let is_horizontal_swipe =
                matches!(gesture, TouchGesture::SlideLeft | TouchGesture::SlideRight);

            // Handle horizontal swipes to switch between widgets
            if is_horizontal_swipe && lift_up {
                match gesture {
                    TouchGesture::SlideLeft => {
                        // Swipe left: show debug log (index 1)
                        if self.current_widget_index == 0 {
                            self.current_widget_index = 1;
                            self.widget.show_logs();
                            frostsnap_widgets::debug::log("Switched to debug log".into());
                        }
                    }
                    TouchGesture::SlideRight => {
                        // Swipe right: show main widget (index 0)
                        if self.current_widget_index == 1 {
                            self.current_widget_index = 0;
                            self.widget.show_main();
                            frostsnap_widgets::debug::log("Switched to main widget".into());
                        }
                    }
                    _ => {}
                }
            }

            // Handle vertical drag for widgets that support it
            if is_vertical_drag {
                self.widget.handle_vertical_drag(
                    self.last_touch.map(|point| point.y as u32),
                    touch_point.y as u32,
                    lift_up,
                );
            }

            if !is_vertical_drag || lift_up {
                // Always handle touch events (for both press and release)
                // This is important so that lift_up is processed after drag
                self.widget.handle_touch(touch_point, now_ms, lift_up);
            }
            // Store last touch for drag calculations
            if lift_up {
                self.last_touch = None;
            } else {
                self.last_touch = Some(touch_point);
            }
        };

        // Only redraw if at least 10ms has passed since last redraw
        let elapsed_ms = (now - self.last_redraw_time).to_millis();
        if elapsed_ms >= 5 {
            // Draw the widget tree
            // Draw the UI stack (includes debug stats overlay)
            let _ = self.widget.draw(&mut self.display, now_ms);

            // Update last redraw time
            self.last_redraw_time = now;
        }

        // Check widget states and generate UI events
        match self.widget.inner_mut().current_mut() {
            WidgetTree::KeygenCheck {
                widget: keygen_check,
                phase,
            } => {
                // Check if confirmed and we still have the phase
                if keygen_check.is_confirmed() {
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
                if sign_prompt.is_confirmed() {
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
            WidgetTree::DisplayBackupRequestPrompt { widget, phase } => {
                // Check if completed and we still have the phase
                if widget.is_completed() {
                    // Take the phase (move it out of the Option)
                    if let Some(phase_data) = phase.take() {
                        return Some(UiEvent::BackupRequestConfirm { phase: phase_data });
                    }
                }
            }
            WidgetTree::EnterBackup { widget, phase } => {
                // Check if backup entry is complete
                if widget.is_finished() {
                    if let Some(share_backup) = widget.get_backup() {
                        if let Some(phase) = phase.take() {
                            return Some(UiEvent::EnteredShareBackup {
                                phase,
                                share_backup,
                            });
                        };
                    }
                }
            }
            WidgetTree::NewNamePrompt { widget, new_name } => {
                // Check if the name prompt was confirmed and we haven't already sent the event
                if widget.is_completed() {
                    if let Some(name) = new_name.take() {
                        return Some(UiEvent::NameConfirm(name));
                    }
                }
            }
            WidgetTree::WipeDevicePrompt { widget, confirmed } => {
                // Check if the wipe device prompt was confirmed and we haven't already sent the event
                if widget.is_completed() && !*confirmed {
                    *confirmed = true;
                    return Some(UiEvent::WipeDataConfirm);
                }
            }
            _ => {}
        }

        None
    }

    fn set_busy_task(&mut self, task: BusyTask) {
        self.busy_task = Some(task);
        // TODO: Update widget tree based on busy task
        self.widget.force_full_redraw();
    }

    fn clear_busy_task(&mut self) {
        self.busy_task = None;
        self.widget.force_full_redraw();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
