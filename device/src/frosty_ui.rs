use crate::DISPLAY_REFRESH_MS;
use alloc::{boxed::Box, string::ToString};
use embedded_graphics::{geometry::Size, prelude::*};
use frostsnap_core::device::card::CardScreen;
use frostsnap_cst816s::interrupt::TouchReceiver;
use frostsnap_widgets::palette::PALETTE;
use frostsnap_widgets::{debug::OverlayDebug, DynWidget, Standby, Widget};

use crate::touch_handler;
use crate::ui::FirmwareUpgradeStatus;
use crate::{
    root_widget::RootWidget, ui::*, widget_tree::WidgetTree, DownstreamConnectionState, Instant,
    UpstreamConnectionState,
};

// Type alias for the display to match factory
type DeviceDisplay<'a> = mipidsi::Display<
    mipidsi::interface::SpiInterface<
        'a,
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
    /// When a touch event was last dequeued.
    pub last_touch_event: Instant,
    pub last_redraw_time: Instant,
    pub downstream_connection_state: DownstreamConnectionState,
    pub upstream_connection_state: Option<UpstreamConnectionState>,
    pub busy_task: Option<BusyTask>,
    pub current_widget_index: usize,
    default_workflow: Option<Workflow>,
    /// The word and undos left of the last device round answer screen, so a second look built
    /// fresh keeps the word's undos.
    round_undos: Option<(u8, u8)>,
}

impl<'a> FrostyUi<'a> {
    /// Create a new FrostyUi instance. Never inlined: the widget-tree
    /// construction is stack-heavy and merging it into the init functions'
    /// frames pushes them over the CI stack-check limit.
    #[inline(never)]
    pub fn new(display: DeviceDisplay<'a>, touch_receiver: TouchReceiver) -> Self {
        use embedded_graphics::geometry::Size;
        use frostsnap_widgets::debug::EnabledDebug;

        let root_widget = RootWidget::new(
            WidgetTree::Standby(Box::new(Standby::new(crate::FIRMWARE_VERSION))),
            200,
        );
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
            last_touch_event: Instant::EPOCH,
            last_redraw_time: Instant::EPOCH,
            current_widget_index: 0,
            busy_task: Default::default(),
            default_workflow: None,
            round_undos: None,
        }
    }
}

/// A touch counts as down for this long after its last event. A finger on the screen keeps
/// sending events, so a touch older than this lost its lift-up to a full queue. Kept from Layer
/// 1 over design §9.3's bare `last_touch.is_some()`: a held finger still counts as down, and a
/// lost lift-up can't stop the card job until the next touch.
const TOUCH_STALE_MS: u64 = 1_000;

impl<'a> FrostyUi<'a> {
    /// A touch is down or queued, a fade is running, or the device round's flash or fade is.
    /// Card jobs skip their slice then, so touches are drained and drawn at the full frame
    /// rate, swipes and taps stay smooth while the proofs and openings run behind the writing
    /// and the quizzes, and a lift-up can't be lost.
    pub fn is_interacting(&self) -> bool {
        let touch_down = self.last_touch.is_some()
            && self.last_touch_event.elapsed().as_millis() < TOUCH_STALE_MS;
        touch_down
            || self.touch_receiver.ready()
            || self.is_transitioning()
            || self.widget.inner().current().is_animating()
    }

    /// A fade between pages is under way.
    pub fn is_transitioning(&self) -> bool {
        self.widget.inner().is_transitioning()
    }

    /// A card screen from a background job may replace what's up: a card session screen, or
    /// the home screen. Any other screen is the user's (a sign prompt, say), and is kept
    /// (design §2.1 item 9).
    pub fn may_show_card_job_screen(&self) -> bool {
        self.at_standby() || self.widget.inner().current().is_card_session_screen()
    }

    /// A consolidation's threshold hold is up and the user hasn't held through it yet.
    pub fn consolidate_hold_pending(&self) -> bool {
        matches!(
            self.widget.inner().current(),
            WidgetTree::ConsolidateThresholdHold { phase: Some(_), .. }
        )
    }

    /// The home screen is up.
    pub fn at_standby(&self) -> bool {
        matches!(self.widget.inner().current(), WidgetTree::Standby(_))
    }

    /// Moves the progress bar of the card busy screen that's up. Any other screen is left
    /// alone: a background job never replaces a screen.
    pub fn set_card_progress(&mut self, progress: f32) {
        if let WidgetTree::CardPreparing { widget, .. } | WidgetTree::CardRestoreChecking(widget) =
            self.widget.inner_mut().current_mut()
        {
            widget.set_progress(progress);
        }
    }

    /// The events of the card screens, and the restore report's pages, which follow each other
    /// without core.
    #[inline(never)]
    fn poll_card_screens(&mut self) -> Option<UiEvent> {
        let next = match self.widget.inner_mut().current_mut() {
            WidgetTree::CardRestoreReport {
                widget,
                page,
                findings,
                decoded,
            } if widget.is_done() => match page.next(findings) {
                Some(next_page) => WidgetTree::build_card_restore_report_page(
                    next_page,
                    *findings,
                    decoded.take()?,
                ),
                None => return decoded.take().map(UiEvent::CardRestoreConfirm),
            },
            current => return current.take_card_event(),
        };
        self.widget.inner_mut().switch_to(next);
        None
    }

    /// Notes the undos left on an answer screen that is about to be replaced.
    fn keep_undos(&mut self) {
        if let WidgetTree::CardDeviceRound(widget) = self.widget.inner().current() {
            self.round_undos = Some((widget.box_index(), widget.undos_left()));
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

    fn set_default_workflow(&mut self, workflow: Workflow) {
        self.default_workflow = Some(workflow);
    }

    fn go_to_default(&mut self) {
        if let Some(workflow) = self.default_workflow.clone() {
            self.set_workflow(workflow);
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
                    card_line,
                },
            ) => {
                standby.set_key_with_card_line(
                    device_name.to_string(),
                    held_share.clone(),
                    *card_line,
                );
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

            // If we're already showing EraseProgress, just update the progress
            (WidgetTree::EraseProgress { widget }, Workflow::EraseProgress { progress }) => {
                widget.update_progress(*progress);
                return;
            }

            // Card screens that update in place: the busy screens keep their progress, the
            // gate moves its dots, and the answer screen moves from word to word. A repeat of
            // words 1-32 or of a quiz that is up redraws nothing, so a resent screen never
            // sends the user back to word 1.
            (
                WidgetTree::CardPreparing { task: shown, .. },
                Workflow::Card(CardScreen::Preparing { task }),
            ) if WidgetTree::same_busy_copy(*shown, *task) => {
                return;
            }
            (WidgetTree::CardRestoreChecking(_), Workflow::Card(CardScreen::RestoreChecking)) => {
                return;
            }
            (
                WidgetTree::CardDeviceHalf { fired: false, .. },
                Workflow::Card(CardScreen::DeviceHalf { .. }),
            )
            | (
                WidgetTree::CardSelfCheck {
                    fired: false,
                    recheck: false,
                    ..
                },
                Workflow::Card(CardScreen::SelfCheck { .. }),
            )
            | (
                WidgetTree::CardSelfCheck {
                    fired: false,
                    recheck: true,
                    ..
                },
                Workflow::Card(CardScreen::Recheck { .. }),
            ) => {
                return;
            }
            (
                WidgetTree::CardGate(widget),
                Workflow::Card(CardScreen::Gate { dots, preparing }),
            ) => {
                widget.set(*dots, *preparing);
                return;
            }
            (
                WidgetTree::CardDeviceRound(widget),
                Workflow::Card(CardScreen::DeviceRoundAnswer {
                    j,
                    k,
                    options,
                    display_id,
                }),
            ) => {
                widget.set_question(*j, *k, options.0, *display_id);
                return;
            }

            // If we're showing KeygenCheck and get another KeyGen prompt, we need a new one
            // because the security code would be different
            _ => {} // Different widget types, need to switch
        };

        // A second look built fresh keeps its word's undos; any other screen ends them
        if matches!(workflow, Workflow::Card(_)) {
            self.keep_undos();
        } else {
            self.round_undos = None;
        }

        // Convert workflow to widget tree
        let new_page = match workflow {
            Workflow::Startup => WidgetTree::build_standby(None, false),
            Workflow::None => WidgetTree::build_standby(None, true),
            Workflow::Standby {
                device_name,
                held_share,
                card_line,
            } => WidgetTree::build_standby(Some((device_name, held_share, card_line)), false),
            Workflow::UserPrompt(prompt) => match prompt {
                Prompt::KeyGen { phase } => WidgetTree::build_keygen_check(phase),
                Prompt::Signing { phase, rand_seed } => {
                    WidgetTree::build_signing_prompt(phase, rand_seed)
                }
                Prompt::ConfirmFirmwareUpgrade {
                    firmware_digest,
                    size,
                } => WidgetTree::build_confirm_firmware_upgrade(firmware_digest, size),
                Prompt::EraseDevice => WidgetTree::build_erase_device(),
                Prompt::ConsolidateThreshold { phase, warning } => {
                    WidgetTree::build_consolidate_threshold_hold(phase, warning)
                }
            },

            Workflow::NamingDevice { new_name } => WidgetTree::build_device_naming(&new_name),

            Workflow::DisplayBackup {
                key_name: _,
                backup,
                access_structure_ref: _,
            } => WidgetTree::build_display_backup(backup),

            Workflow::CheckBackup {
                key_name: _,
                backup,
                access_structure_ref,
                rand_seed,
            } => WidgetTree::build_check_backup(backup, access_structure_ref, rand_seed),

            Workflow::EnteringBackup(phase) => WidgetTree::build_entering_backup(phase),

            Workflow::DisplayAddress {
                address,
                bip32_path,
                rand_seed,
            } => WidgetTree::build_display_address(address, bip32_path, rand_seed),

            Workflow::FirmwareUpgrade(status) => WidgetTree::build_firmware_upgrade(status),

            Workflow::EraseProgress { progress } => WidgetTree::build_erase_progress(progress),

            // by reference, so the words are wiped where core put them
            Workflow::Card(screen) => {
                match WidgetTree::build_card_screen(&screen, self.round_undos) {
                    Some(page) => page,
                    None => return,
                }
            }
            Workflow::CardRestoreEntry { phase, review } => {
                WidgetTree::build_card_restore_entry(phase, review.as_ref())
            }
            // by reference, so the share is wiped in the box it came in
            Workflow::CardRestoreReport(report) => WidgetTree::build_card_restore_report(&report),
        };

        // Switch to the new page with fade transition
        self.widget.inner_mut().switch_to(new_page);
    }

    fn poll(&mut self) -> Option<UiEvent> {
        self.widget
            .inner_mut()
            .current_mut()
            .build_pending(Size::new(240, 280));
        let now = Instant::now();
        let now_ms =
            frostsnap_widgets::Instant::from_millis(now.duration_since_epoch().as_millis());

        // Handle touch input
        if self.touch_receiver.ready() {
            self.last_touch_event = now;
        }
        touch_handler::process_all_touch_events(
            &mut self.touch_receiver,
            &mut self.widget,
            &mut self.last_touch,
            &mut self.current_widget_index,
            now_ms,
        );

        // Only redraw if enough time has passed since last redraw
        let elapsed_ms = (now - self.last_redraw_time).as_millis();
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
            } if keygen_check.is_confirmed() => {
                // Take the phase (move it out of the Option)
                if let Some(phase_data) = phase.take() {
                    return Some(UiEvent::KeyGenConfirm { phase: phase_data });
                }
            }
            WidgetTree::SignTxPrompt {
                widget: sign_prompt,
                phase,
            } => {
                if let Some(phase_data) = phase.take_if(|_| sign_prompt.is_confirmed()) {
                    return Some(UiEvent::SigningConfirm { phase: phase_data });
                }
                if phase.is_none() && sign_prompt.is_finished() {
                    self.go_to_default();
                }
            }
            WidgetTree::SignTestPrompt { widget, phase } => {
                if let Some(phase_data) = phase.take_if(|_| widget.is_confirmed()) {
                    return Some(UiEvent::SigningConfirm { phase: phase_data });
                }
                if phase.is_none() && widget.is_finished() {
                    self.go_to_default();
                }
            }
            WidgetTree::FirmwareUpgradeConfirm {
                widget, confirmed, ..
            } if widget.is_confirmed() && !*confirmed => {
                *confirmed = true;
                return Some(UiEvent::UpgradeConfirm);
            }
            WidgetTree::DisplayBackup { widget, fired } => {
                if !widget.is_finished() || *fired {
                    return None;
                }
                *fired = true;
                return Some(UiEvent::BackupRecorded);
            }
            WidgetTree::CheckBackup {
                widget,
                access_structure_ref,
                share_index,
                fired,
            } => {
                if widget.is_verified() && !*fired {
                    *fired = true;
                    return Some(UiEvent::BackupChecked {
                        access_structure_ref: *access_structure_ref,
                        share_index: *share_index,
                    });
                }
                if *fired && widget.is_finished() {
                    self.go_to_default();
                }
            }
            WidgetTree::EnterBackup { widget, phase } => {
                if !widget.is_finished() {
                    return None;
                }
                let share_backup = widget.get_backup()?;
                let phase = phase.take()?;
                return Some(UiEvent::EnteredShareBackup {
                    phase,
                    share_backup,
                });
            }
            WidgetTree::EraseDevicePrompt { widget, confirmed } => {
                if !widget.is_confirmed() || *confirmed {
                    return None;
                }
                *confirmed = true;
                return Some(UiEvent::EraseDataConfirm);
            }
            WidgetTree::ConsolidateThresholdHold { widget, phase } => {
                if let Some(phase) = phase.take_if(|_| widget.is_done()) {
                    return Some(UiEvent::ConsolidateConfirm { phase });
                }
            }
            _ => {}
        }

        self.poll_card_screens()
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
        let now = Instant::now();
        let now_ms =
            frostsnap_widgets::Instant::from_millis(now.duration_since_epoch().as_millis());
        self.last_redraw_time = now;
        let _ = self.widget.draw(&mut self.display, now_ms);
    }
}
