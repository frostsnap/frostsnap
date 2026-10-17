use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics_simulator::SimulatorDisplay;
use frostsnap_core::device::{KeyGenPhase3, SignPhase1, SpEcdhPhase};
use frostsnap_core::message::HeldShare2;
use frostsnap_widgets::palette::PALETTE;
use frostsnap_widgets::{
    BackupDisplay, DefaultTextStyle, DeviceNameScreen, DynWidget, FONT_MED,
    HOLD_TO_CONFIRM_TIME_MS, HoldToConfirm, Instant, KeygenCheck, SignMessageConfirm,
    SignPromptKind, SignTxPrompt, Standby, SuperDrawTarget, Text, Widget,
};

pub enum SimUiEvent {
    KeyGenConfirm(KeyGenPhase3),
    SigningConfirm(SignPhase1),
    SpEcdhConfirm(SpEcdhPhase),
    NameConfirm(String),
    BackupRecorded,
}

pub enum ActiveScreen {
    Standby(Box<Standby>),
    NamingDevice(Box<DeviceNameScreen>),
    NewNamePrompt {
        widget: Box<HoldToConfirm<Text>>,
        new_name: Option<String>,
    },
    KeygenCheck {
        widget: Box<KeygenCheck>,
        phase: Option<KeyGenPhase3>,
    },
    DisplayBackup {
        widget: Box<BackupDisplay>,
        fired: bool,
    },
    SignTxPrompt {
        widget: Box<SignTxPrompt>,
        phase: Option<SignPhase1>,
    },
    SpEcdhPrompt {
        widget: Box<SignTxPrompt>,
        phase: Option<SpEcdhPhase>,
    },
    SignTestPrompt {
        widget: Box<SignMessageConfirm>,
        phase: Option<SignPhase1>,
    },
    Unsupported {
        widget: Box<Text>,
    },
}

impl ActiveScreen {
    pub fn new_standby(device_name: Option<&str>, held_share: Option<HeldShare2>) -> Self {
        let mut standby = Standby::new();
        if let (Some(name), Some(share)) = (device_name, held_share) {
            standby.set_key(name, share);
            standby.set_constraints(Size::new(240, 280));
            ActiveScreen::Standby(Box::new(standby))
        } else if let Some(name) = device_name {
            let mut screen = DeviceNameScreen::new(name.to_string());
            screen.set_constraints(Size::new(240, 280));
            ActiveScreen::NamingDevice(Box::new(screen))
        } else {
            standby.set_welcome();
            standby.set_constraints(Size::new(240, 280));
            ActiveScreen::Standby(Box::new(standby))
        }
    }

    pub fn new_naming(name: &str) -> Self {
        let mut screen = DeviceNameScreen::new(name.to_string());
        screen.set_constraints(Size::new(240, 280));
        ActiveScreen::NamingDevice(Box::new(screen))
    }

    pub fn new_name_prompt(old_name: Option<&str>, new_name: &str) -> Self {
        let prompt_text = if let Some(old) = old_name {
            format!("Rename device\nfrom '{}'\nto '{}'?", old, new_name)
        } else {
            format!("Name device\n'{}'?", new_name)
        };
        let text_widget = Text::new(
            prompt_text,
            DefaultTextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(embedded_graphics::text::Alignment::Center);
        let mut hold_to_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, text_widget);
        hold_to_confirm.set_constraints(Size::new(240, 280));
        ActiveScreen::NewNamePrompt {
            widget: Box::new(hold_to_confirm),
            new_name: Some(new_name.to_string()),
        }
    }

    pub fn new_keygen(phase: KeyGenPhase3) -> Self {
        let session_hash = phase.session_hash();
        let mut security_check_code = [0u8; 4];
        security_check_code.copy_from_slice(&session_hash.0[..4]);
        let mut widget = KeygenCheck::new(phase.t_of_n(), security_check_code);
        widget.set_constraints(Size::new(240, 280));
        ActiveScreen::KeygenCheck {
            widget: Box::new(widget),
            phase: Some(phase),
        }
    }

    pub fn new_display_backup(backup: &frost_backup::ShareBackup) -> Self {
        let word_indices = backup.to_word_indices();
        let share_index: u16 = backup.index().try_into().expect("share index fits in u16");
        let mut widget = BackupDisplay::new(word_indices, share_index);
        widget.set_constraints(Size::new(240, 280));
        ActiveScreen::DisplayBackup {
            widget: Box::new(widget),
            fired: false,
        }
    }

    pub fn new_signing(phase: SignPhase1, rand_seed: u32, list_threshold: usize) -> Self {
        let sign_task = phase.sign_task();
        match &sign_task.inner {
            frostsnap_core::SignTask::BitcoinTransaction {
                tx_template,
                network,
            } => {
                let prompt = tx_template.user_prompt(*network);
                let mut widget = Box::new(SignTxPrompt::new_with_seed_and_kind(
                    prompt,
                    rand_seed,
                    SignPromptKind::Sign,
                    list_threshold,
                ));
                widget.set_constraints(Size::new(240, 280));
                ActiveScreen::SignTxPrompt {
                    widget,
                    phase: Some(phase),
                }
            }
            frostsnap_core::SignTask::Test { message } => {
                let mut widget = Box::new(SignMessageConfirm::new(message.clone()));
                widget.set_constraints(Size::new(240, 280));
                ActiveScreen::SignTestPrompt {
                    widget,
                    phase: Some(phase),
                }
            }
            _ => {
                eprintln!("unsupported workflow: sign task not supported");
                Self::new_unsupported("Unsupported sign task")
            }
        }
    }

    pub fn new_sp_ecdh(phase: SpEcdhPhase, rand_seed: u32, list_threshold: usize) -> Self {
        let sign_task = phase.sign_task();
        match &sign_task.inner {
            frostsnap_core::SignTask::BitcoinTransaction {
                tx_template,
                network,
            } => {
                let prompt = tx_template.user_prompt(*network);
                let mut widget = Box::new(SignTxPrompt::new_with_seed_and_kind(
                    prompt,
                    rand_seed,
                    SignPromptKind::SpEcdh,
                    list_threshold,
                ));
                widget.set_constraints(Size::new(240, 280));
                ActiveScreen::SpEcdhPrompt {
                    widget,
                    phase: Some(phase),
                }
            }
            _ => {
                eprintln!("unsupported workflow: SP ECDH sign task must be BitcoinTransaction");
                Self::new_unsupported("Unsupported SP ECDH task")
            }
        }
    }

    pub fn new_unsupported(msg: &str) -> Self {
        let mut text_widget = Text::new(
            msg.to_string(),
            DefaultTextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(embedded_graphics::text::Alignment::Center);
        text_widget.set_constraints(Size::new(240, 280));
        ActiveScreen::Unsupported {
            widget: Box::new(text_widget),
        }
    }

    pub fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) {
        match self {
            ActiveScreen::Standby(w) => {
                let _ = w.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::NamingDevice(w) => {
                let _ = w.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::NewNamePrompt { widget, .. } => {
                let _ = widget.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::KeygenCheck { widget, .. } => {
                let _ = widget.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::DisplayBackup { widget, .. } => {
                let _ = widget.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::SignTxPrompt { widget, .. } => {
                let _ = widget.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::SpEcdhPrompt { widget, .. } => {
                let _ = widget.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::SignTestPrompt { widget, .. } => {
                let _ = widget.handle_touch(point, current_time, is_release);
            }
            ActiveScreen::Unsupported { widget } => {
                let _ = widget.handle_touch(point, current_time, is_release);
            }
        }
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, current_y: u32, is_release: bool) {
        match self {
            ActiveScreen::Standby(w) => {
                w.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::NamingDevice(w) => {
                w.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::NewNamePrompt { widget, .. } => {
                widget.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::KeygenCheck { widget, .. } => {
                widget.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::DisplayBackup { widget, .. } => {
                widget.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::SignTxPrompt { widget, .. } => {
                widget.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::SpEcdhPrompt { widget, .. } => {
                widget.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::SignTestPrompt { widget, .. } => {
                widget.handle_vertical_drag(prev_y, current_y, is_release);
            }
            ActiveScreen::Unsupported { widget } => {
                widget.handle_vertical_drag(prev_y, current_y, is_release);
            }
        }
    }

    pub fn draw(
        &mut self,
        target: &mut SuperDrawTarget<SimulatorDisplay<Rgb565>, Rgb565>,
        current_time: Instant,
    ) {
        match self {
            ActiveScreen::Standby(w) => {
                let _ = w.draw(target, current_time);
            }
            ActiveScreen::NamingDevice(w) => {
                let _ = w.draw(target, current_time);
            }
            ActiveScreen::NewNamePrompt { widget, .. } => {
                let _ = widget.draw(target, current_time);
            }
            ActiveScreen::KeygenCheck { widget, .. } => {
                let _ = widget.draw(target, current_time);
            }
            ActiveScreen::DisplayBackup { widget, .. } => {
                let _ = widget.draw(target, current_time);
            }
            ActiveScreen::SignTxPrompt { widget, .. } => {
                let _ = widget.draw(target, current_time);
            }
            ActiveScreen::SpEcdhPrompt { widget, .. } => {
                let _ = widget.draw(target, current_time);
            }
            ActiveScreen::SignTestPrompt { widget, .. } => {
                let _ = widget.draw(target, current_time);
            }
            ActiveScreen::Unsupported { widget } => {
                let _ = widget.draw(target, current_time);
            }
        }
    }

    pub fn poll(&mut self) -> Option<SimUiEvent> {
        match self {
            ActiveScreen::KeygenCheck { widget, phase } => {
                if widget.is_confirmed() {
                    return phase.take().map(SimUiEvent::KeyGenConfirm);
                }
            }
            ActiveScreen::NewNamePrompt { widget, new_name } => {
                if widget.is_confirmed() {
                    return new_name.take().map(SimUiEvent::NameConfirm);
                }
            }
            ActiveScreen::SignTxPrompt { widget, phase } => {
                if widget.is_confirmed() {
                    return phase.take().map(SimUiEvent::SigningConfirm);
                }
            }
            ActiveScreen::SpEcdhPrompt { widget, phase } => {
                if widget.is_confirmed() {
                    return phase.take().map(SimUiEvent::SpEcdhConfirm);
                }
            }
            ActiveScreen::SignTestPrompt { widget, phase } => {
                if widget.is_confirmed() {
                    return phase.take().map(SimUiEvent::SigningConfirm);
                }
            }
            ActiveScreen::DisplayBackup { widget, fired } => {
                if widget.is_finished() && !*fired {
                    *fired = true;
                    return Some(SimUiEvent::BackupRecorded);
                }
            }
            _ => {}
        }
        None
    }

    pub fn is_finished(&mut self) -> bool {
        match self {
            ActiveScreen::SignTxPrompt { widget, phase } => phase.is_none() && widget.is_finished(),
            ActiveScreen::SpEcdhPrompt { widget, phase } => phase.is_none() && widget.is_finished(),
            ActiveScreen::SignTestPrompt { widget, phase } => {
                phase.is_none() && widget.is_finished()
            }
            ActiveScreen::DisplayBackup { fired, .. } => *fired,
            _ => false,
        }
    }
}
