use crate::DISPLAY_REFRESH_MS;
use alloc::{boxed::Box, string::ToString};
use embedded_graphics::prelude::*;
use esp_hal::prelude::*;
use frostsnap_cst816s::interrupt::TouchReceiver;
use frostsnap_widgets::palette::PALETTE;
use frostsnap_widgets::{
    backup::{BackupDisplay, EnterShareScreen},
    debug::OverlayDebug,
    keygen_check::KeygenCheck,
    sign_prompt::SignTxPrompt,
    DeviceNameScreen, DynWidget, FirmwareUpgradeConfirm, FirmwareUpgradeProgress, Standby, Widget,
    HOLD_TO_CONFIRM_TIME_LONG_MS, HOLD_TO_CONFIRM_TIME_MS,
};

use crate::touch_handler;
use crate::ui::FirmwareUpgradeStatus;
use crate::{
    root_widget::RootWidget, ui::*, widget_tree::WidgetTree, DownstreamConnectionState, Instant,
    UpstreamConnectionState,
};

// Type alias for the display to match factory
type DeviceDisplay<'a> = mipidsi::Display<
    display_interface_spi::SPIInterface<
        embedded_hal_bus::spi::ExclusiveDevice<
            esp_hal::spi::master::Spi<'a, esp_hal::Blocking>,
            crate::peripherals::NoCs,
            embedded_hal_bus::spi::NoDelay,
        >,
        esp_hal::gpio::Output<'a>,
    >,
    mipidsi::models::ST7789,
    esp_hal::gpio::Output<'a>,
>;

pub struct FrostyUi<'a> {
    pub display: frostsnap_widgets::SuperDrawTarget<
        DeviceDisplay<'a>,
        embedded_graphics::pixelcolor::Rgb565,
    >,
    pub widget: OverlayDebug<RootWidget>,
    pub touch_receiver: TouchReceiver,
    pub last_touch: Option<Point>,
    pub last_redraw_time: Instant,
    pub downstream_connection_state: DownstreamConnectionState,
    pub upstream_connection_state: Option<UpstreamConnectionState>,
    pub timer: esp_hal::timer::timg::Timer<
        esp_hal::timer::timg::Timer0<esp_hal::peripherals::TIMG1>,
        esp_hal::Blocking,
    >,
    pub busy_task: Option<BusyTask>,
    pub current_widget_index: usize,
}

impl<'a> FrostyUi<'a> {
    /// Create a new FrostyUi instance
    pub fn new(
        display: DeviceDisplay<'a>,
        touch_receiver: TouchReceiver,
        timer: esp_hal::timer::timg::Timer<
            esp_hal::timer::timg::Timer0<esp_hal::peripherals::TIMG1>,
            esp_hal::Blocking,
        >,
    ) -> Self {
        use embedded_graphics::geometry::Size;
        use frostsnap_widgets::debug::EnabledDebug;

        let root_widget = RootWidget::new(WidgetTree::Standby(Box::new(Standby::new())), 200);
        let debug_config = EnabledDebug {
            logs: cfg!(feature = "debug_log"),
            memory: cfg!(feature = "debug_mem"),
            fps: cfg!(feature = "debug_fps"),
        };
        let mut widget_with_debug = OverlayDebug::new(root_widget, debug_config);
        widget_with_debug.set_constraints(Size::new(240, 280));

        Self {
            display: frostsnap_widgets::SuperDrawTarget::new(display, PALETTE.background),
            widget: widget_with_debug,
            touch_receiver,
            downstream_connection_state: DownstreamConnectionState::Disconnected,
            upstream_connection_state: None,
            last_touch: None,
            last_redraw_time: Instant::from_ticks(0),
            current_widget_index: 0,
            timer,
            busy_task: Default::default(),
        }
    }
}

impl<'a> UserInteraction for FrostyUi<'a> {
    fn set_downstream_connection_state(&mut self, state: crate::DownstreamConnectionState) {
        if state != self.downstream_connection_state {
            self.downstream_connection_state = state;
        }
    }

    fn set_upstream_connection_state(&mut self, state: crate::UpstreamConnectionState) {
        if Some(state) != self.upstream_connection_state {
            self.upstream_connection_state = Some(state);
        }
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        // Check if we can update the current widget instead of switching
        let current_widget = self.widget.inner_mut().current_mut();

        match (current_widget, &workflow) {
            // If we're already showing Standby, just update its mode
            (WidgetTree::Standby(ref mut standby), Workflow::Startup) => {
                standby.clear_content();
                return;
            }
            (WidgetTree::Standby(ref mut standby), Workflow::None) => {
                standby.set_welcome();
                return;
            }
            (
                WidgetTree::Standby(ref mut standby),
                Workflow::Standby {
                    device_name,
                    held_share,
                },
            ) => {
                standby.set_key(device_name.to_string(), held_share.clone());
                return;
            }

            // If we're already showing DeviceNaming and get another NamingDevice workflow, just update the text
            (
                WidgetTree::DeviceNaming(ref mut device_name_screen),
                Workflow::NamingDevice { ref new_name },
            ) => {
                device_name_screen.set_name(new_name.to_string());
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
            Workflow::Startup => WidgetTree::Standby(Box::new(Standby::new())),
            Workflow::None => {
                let mut standby = Standby::new();
                standby.set_welcome();
                WidgetTree::Standby(Box::new(standby))
            }
            Workflow::Standby {
                device_name,
                held_share,
            } => {
                let mut standby = Standby::new();
                standby.set_key(device_name.to_string(), held_share);
                WidgetTree::Standby(Box::new(standby))
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

                                // Create the SignTxPrompt widget
                                let widget = Box::new(SignTxPrompt::new(prompt));

                                // Store both widget and phase in the WidgetTree
                                WidgetTree::SignTxPrompt {
                                    widget,
                                    phase: Some(phase),
                                }
                            }
                            frostsnap_core::SignTask::Test { message } => {
                                use frostsnap_widgets::SignMessageConfirm;

                                let widget = Box::new(SignMessageConfirm::new(message.clone()));

                                WidgetTree::SignTestPrompt {
                                    widget,
                                    phase: Some(phase),
                                }
                            }
                            frostsnap_core::SignTask::Nostr { .. } => {
                                // Nostr signing not implemented yet
                                let mut standby = Standby::new();
                                standby.set_welcome();
                                WidgetTree::Standby(Box::new(standby))
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
                    Prompt::NewName { old_name, new_name } => {
                        use frostsnap_widgets::DefaultTextStyle;
                        use frostsnap_widgets::{HoldToConfirm, Text, FONT_MED};

                        // Create text for the prompt
                        let prompt_text = if let Some(old_name) = old_name {
                            format!("Rename device\nfrom '{}'\nto '{}'?", old_name, new_name)
                        } else {
                            format!("Name device\n'{}'?", new_name)
                        };

                        let text_widget = Text::new(
                            prompt_text,
                            DefaultTextStyle::new(FONT_MED, PALETTE.on_background),
                        )
                        .with_alignment(embedded_graphics::text::Alignment::Center);

                        // Create HoldToConfirm widget with 2 second hold time
                        let hold_to_confirm =
                            HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, text_widget);

                        WidgetTree::NewNamePrompt {
                            widget: Box::new(hold_to_confirm),
                            new_name: Some(new_name.clone()),
                        }
                    }
                    Prompt::WipeDevice => {
                        use frostsnap_widgets::DefaultTextStyle;
                        use frostsnap_widgets::{HoldToConfirm, Text, FONT_MED};

                        // Create warning text for device wipe
                        let prompt_text = "WARNING!\n\nErase all data?\n\nHold to confirm";

                        let text_widget =
                            Text::new(prompt_text, DefaultTextStyle::new(FONT_MED, PALETTE.error))
                                .with_alignment(embedded_graphics::text::Alignment::Center);

                        // Create HoldToConfirm widget with 3 second hold time for wipe
                        let hold_to_confirm =
                            HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_LONG_MS, text_widget);

                        WidgetTree::WipeDevicePrompt {
                            widget: Box::new(hold_to_confirm),
                            confirmed: false,
                        }
                    }
                }
            }

            Workflow::NamingDevice { new_name } => {
                let device_name_screen = DeviceNameScreen::new(new_name.to_string());
                WidgetTree::DeviceNaming(Box::new(device_name_screen))
            }

            Workflow::DisplayBackup {
                key_name: _,
                backup,
                access_structure_ref,
            } => {
                let word_indices = backup.to_word_indices();
                let share_index: u16 = backup
                    .index()
                    .try_into()
                    .expect("Share index should fit in u16");
                let backup_display = BackupDisplay::new(word_indices, share_index);
                WidgetTree::DisplayBackup {
                    widget: Box::new(backup_display),
                    access_structure_ref: Some(access_structure_ref),
                }
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
                rand_seed,
            } => {
                use frostsnap_widgets::{AddressWithPath, Center};

                // Create the address display widget with just the address index
                let mut address_display = AddressWithPath::new(address, bip32_path.index);
                address_display.set_rand_highlight(rand_seed);
                WidgetTree::AddressDisplay(Box::new(Center::new(address_display)))
            }

            Workflow::FirmwareUpgrade(status) => {
                use crate::ui::FirmwareUpgradeStatus;

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
        touch_handler::process_all_touch_events(
            &mut self.touch_receiver,
            &mut self.widget,
            &mut self.last_touch,
            &mut self.current_widget_index,
            now_ms,
        );

        // Only redraw if enough time has passed since last redraw
        let elapsed_ms = (now - self.last_redraw_time).to_millis();
        if elapsed_ms >= DISPLAY_REFRESH_MS {
            // Update last redraw time
            self.last_redraw_time = now;
            // Draw the widget tree
            // Draw the UI stack (includes debug stats overlay)
            let _ = self.widget.draw(&mut self.display, now_ms);
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
            WidgetTree::SignTxPrompt {
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
            WidgetTree::SignTestPrompt { widget, phase } => {
                // Check if confirmed and we still have the phase
                if widget.is_confirmed() {
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
            WidgetTree::DisplayBackup {
                widget,
                access_structure_ref,
            } => {
                if widget.is_confirmed() {
                    if let Some(access_structure_ref_data) = access_structure_ref.take() {
                        return Some(UiEvent::BackupRecorded {
                            access_structure_ref: access_structure_ref_data,
                        });
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

    fn force_redraw(&mut self) {
        let now = self.timer.now();
        let now_ms =
            frostsnap_widgets::Instant::from_millis(now.duration_since_epoch().to_millis());
        self.last_redraw_time = now;
        let _ = self.widget.draw(&mut self.display, now_ms);
    }
}
