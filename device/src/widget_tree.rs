use alloc::boxed::Box;
use frostsnap_core::device::{
    restoration::{BackupDisplayPhase, EnterBackupPhase},
    KeyGenPhase3, SignPhase1,
};
use frostsnap_widgets::{
    backup::{BackupDisplay, EnterShareScreen},
    keygen_check::KeygenCheck,
    layout::*,
    sign_prompt::SignTxPrompt,
    DeviceNameScreen, FirmwareUpgradeConfirm, FirmwareUpgradeProgress, HoldToConfirm,
    SignMessageConfirm, Standby, Text,
};

use crate::ui::FirmwareUpgradeStatus;

// Type alias for the backup request prompt widget
type BackupRequestPromptWidget =
    HoldToConfirm<Center<frostsnap_widgets::Column<(Text, Text, Text)>>>;

/// The widget tree represents the current UI state as a tree of widgets
#[derive(frostsnap_macros::Widget)]
#[widget_crate(frostsnap_widgets)]
pub enum WidgetTree {
    /// Standby screen (can show startup/empty, welcome, or key info)
    Standby(Box<Standby>),

    /// Device naming screen
    DeviceNaming(Box<DeviceNameScreen>),

    /// Keygen confirmation screen
    KeygenCheck {
        widget: Box<KeygenCheck>,
        phase: Option<Box<KeyGenPhase3>>,
    },

    /// Sign transaction prompt screen
    SignTxPrompt {
        widget: Box<SignTxPrompt>,
        phase: Option<Box<SignPhase1>>,
    },

    /// Sign test message prompt screen
    SignTestPrompt {
        widget: Box<SignMessageConfirm>,
        phase: Option<Box<SignPhase1>>,
    },

    /// Firmware upgrade confirmation screen
    FirmwareUpgradeConfirm {
        widget: Box<FirmwareUpgradeConfirm>,
        firmware_hash: [u8; 32],
        firmware_size: u32,
        confirmed: bool,
    },

    /// Firmware upgrade progress screen
    FirmwareUpgradeProgress {
        widget: Box<FirmwareUpgradeProgress>,
        status: FirmwareUpgradeStatus,
    },

    /// Display backup request prompt
    DisplayBackupRequestPrompt {
        widget: Box<BackupRequestPromptWidget>,
        phase: Option<Box<BackupDisplayPhase>>,
    },

    /// New name confirmation prompt
    NewNamePrompt {
        widget: Box<HoldToConfirm<Text>>,
        new_name: Option<frostsnap_comms::DeviceName>,
    },

    /// Device wipe confirmation prompt  
    WipeDevicePrompt {
        widget: Box<HoldToConfirm<Text>>,
        confirmed: bool,
    },

    /// Display backup screen
    DisplayBackup(Box<BackupDisplay>),

    /// Display Bitcoin address screen with derivation path
    AddressDisplay(Box<Center<frostsnap_widgets::AddressWithPath>>),

    /// Enter backup screen
    EnterBackup {
        widget: Box<EnterShareScreen>,
        phase: Option<EnterBackupPhase>,
    },
}

impl Default for WidgetTree {
    fn default() -> Self {
        WidgetTree::Standby(Box::new(Standby::new()))
    }
}
