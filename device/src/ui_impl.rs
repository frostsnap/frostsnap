//! UI implementation for Frostsnap device

use crate::{
    calibrate_point,
    graphics::{
        self,
        animation::AnimationProgress,
        widgets::{EnterShareIndexScreen, EnterShareScreen},
    },
    ui::{
        BusyTask, EnteringBackupStage, FirmwareUpgradeStatus, Prompt, UiEvent, UserInteraction,
        WaitingFor, WaitingResponse, Workflow,
    },
    DownstreamConnectionState, Instant, UpstreamConnectionState,
};
use alloc::string::String;
use core::borrow::BorrowMut;
use cst816s::TouchGesture;
use embedded_graphics::prelude::*;
use esp_hal::timer::Timer as TimerTrait;
use frostsnap_core::{hex, SignTask};

/// Frostsnap UI implementation
pub struct FrostyUi<'a> {
    display: graphics::Graphics<DeviceDisplay<'a>>,
    capsense: cst816s::CST816S<
        esp_hal::i2c::master::I2c<'a, esp_hal::Blocking>,
        esp_hal::gpio::Input<'a>,
        esp_hal::gpio::Output<'a>,
    >,
    timer: esp_hal::timer::timg::Timer<
        esp_hal::timer::timg::Timer0<esp_hal::peripherals::TIMG1>,
        esp_hal::Blocking,
    >,
    last_touch: Option<(Point, Instant)>,
    downstream_connection_state: DownstreamConnectionState,
    upstream_connection_state: Option<UpstreamConnectionState>,
    workflow: Workflow,
    device_name: Option<String>,
    changes: bool,
    busy_task: Option<BusyTask>,
    recovery_mode: bool,
}

// Type alias for the display type
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

impl<'a> FrostyUi<'a> {
    /// Create a new UI instance
    pub fn new(
        display: DeviceDisplay<'a>,
        capsense: cst816s::CST816S<
            esp_hal::i2c::master::I2c<'a, esp_hal::Blocking>,
            esp_hal::gpio::Input<'a>,
            esp_hal::gpio::Output<'a>,
        >,
        timer: esp_hal::timer::timg::Timer<
            esp_hal::timer::timg::Timer0<esp_hal::peripherals::TIMG1>,
            esp_hal::Blocking,
        >,
    ) -> Self {
        let mut display = graphics::Graphics::new(display);

        // Show initial header
        display.header("Frostsnap");
        display.flush();

        Self {
            display,
            capsense,
            timer,
            last_touch: None,
            downstream_connection_state: DownstreamConnectionState::Disconnected,
            upstream_connection_state: None,
            workflow: Default::default(),
            device_name: Default::default(),
            changes: false,
            busy_task: Default::default(),
            recovery_mode: false,
        }
    }

    fn render(&mut self) {
        if !matches!(self.workflow, Workflow::EnteringBackup { .. }) {
            self.display.clear();
        }
        self.display
            .header(self.device_name.as_deref().unwrap_or("New Device"));

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
                // This is drawn during poll
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

impl<'a> UserInteraction for FrostyUi<'a> {
    fn set_downstream_connection_state(&mut self, state: DownstreamConnectionState) {
        if state != self.downstream_connection_state {
            self.changes = true;
            self.downstream_connection_state = state;
        }
    }

    fn set_upstream_connection_state(&mut self, state: UpstreamConnectionState) {
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
        // Keep the timer register fresh
        let now = self.timer.now();

        let mut event = None;

        let (current_touch, last_touch) = match self.capsense.read_one_touch_event(true) {
            Some(touch) => {
                let corrected_point = calibrate_point(Point::new(touch.x, touch.y));

                let lift_up = touch.action == 1;
                let last_touch = self.last_touch.take();
                if !lift_up {
                    self.last_touch = Some((corrected_point, now));
                }

                (Some((corrected_point, touch.gesture, lift_up)), last_touch)
            }
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
                                            // For now we just make user keep going until they make it right
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
        // HACK: we only display busy task when workflow is None
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
