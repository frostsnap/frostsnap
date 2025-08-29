use alloc::{boxed::Box, string::String};
use embedded_graphics::pixelcolor::Rgb565;
use frostsnap_core::device::{
    restoration::{BackupDisplayPhase, EnterBackupPhase},
    KeyGenPhase2, SignPhase1,
};
use frostsnap_widgets::{
    backup::{BackupDisplay, EnterShareScreen},
    keygen_check::KeygenCheck,
    sign_prompt::SignPrompt,
    DeviceNameScreen, FirmwareUpgradeConfirm, FirmwareUpgradeProgress, HoldToConfirm, Standby,
    Text, Welcome,
};
use u8g2_fonts::U8g2TextStyle;

use crate::ui::FirmwareUpgradeStatus;

// Type alias for text widgets with color mapping
// TODO: Fix this when Text widget is properly implemented with a concrete font type
// type TextWidget = ColorMap<Text<SomeFont>, Rgb565>;

// TODO: Re-enable when implementing backup entry
// // Forward-declare the stage enum for clarity
// pub enum EnteringBackupStage {
//     ShareIndex(EnterShareIndexScreen),
//     Share(EnterShareScreen),
// }

/// The widget tree represents the current UI state as a tree of widgets
#[derive(frostsnap_macros::Widget)]
#[widget_crate(frostsnap_widgets)]
pub enum WidgetTree {
    /// Default welcome screen
    Welcome(Box<Welcome>),

    /// Standby screen showing key and device name
    Standby(Box<Standby>),

    // TODO: Re-enable when Text widget is properly implemented
    // /// Simple text display
    // Text(TextWidget),

    // /// Interactive prompts with hold-to-confirm
    // Prompt {
    //     widget: HoldToConfirm<TextWidget, TextWidget>,
    //     data: Prompt, // The associated prompt data
    // },
    /// Device naming screen
    DeviceNaming(Box<DeviceNameScreen>),

    /// Keygen confirmation screen
    KeygenCheck {
        widget: Box<KeygenCheck>,
        phase: Option<Box<KeyGenPhase2>>,
    },

    /// Sign transaction prompt screen
    SignPrompt {
        widget: Box<SignPrompt>,
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
        widget: Box<HoldToConfirm<Text<U8g2TextStyle<Rgb565>>>>,
        phase: Option<Box<BackupDisplayPhase>>,
    },

    /// New name confirmation prompt
    NewNamePrompt {
        widget: Box<HoldToConfirm<Text<U8g2TextStyle<Rgb565>>>>,
        new_name: Option<String>,
    },

    /// Device wipe confirmation prompt  
    WipeDevicePrompt {
        widget: Box<HoldToConfirm<Text<U8g2TextStyle<Rgb565>>>>,
        confirmed: bool,
    },

    /// Display backup screen
    DisplayBackup(Box<BackupDisplay>),

    /// Display Bitcoin address screen with derivation path
    AddressDisplay(Box<frostsnap_widgets::AddressWithPath>),

    /// Enter backup screen
    EnterBackup {
        widget: Box<EnterShareScreen>,
        phase: Option<EnterBackupPhase>,
    },
    // TODO: Implement these widgets
    // Ready(ReadyScreen),
    // KeyGenPendingFinalize(KeyGenPendingFinalizeScreen),
    // Address(AddressScreen),
    // DisplayBackup(BackupScreen),
}

impl Default for WidgetTree {
    fn default() -> Self {
        WidgetTree::Welcome(Box::new(Welcome::new()))
    }
}
