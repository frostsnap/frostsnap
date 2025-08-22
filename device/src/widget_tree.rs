use alloc::boxed::Box;
use frostsnap_core::device::{KeyGenPhase2, SignPhase1};
use frostsnap_embedded_widgets::{
    keygen_check::KeygenCheck, sign_prompt::SignPrompt, DeviceNameScreen, FirmwareUpgradeConfirm,
    FirmwareUpgradeProgress, Standby, Welcome,
};

use crate::ui::FirmwareUpgradeStatus;
// TODO: Re-enable when implementing backup entry
// use frostsnap_core::device::restoration::EnterBackupPhase;
// use frostsnap_embedded_widgets::legacy::{EnterShareIndexScreen, EnterShareScreen};

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
#[widget_crate(frostsnap_embedded_widgets)]
pub enum WidgetTree {
    /// Default welcome screen
    Welcome(Welcome),

    /// Standby screen showing key and device name
    Standby(Standby),

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
    // TODO: Re-enable when EnterShareIndexScreen and EnterShareScreen implement Widget trait
    // /// Complex interactive screens for entering backup
    // EnterBackup {
    //     stage: EnteringBackupStage,
    //     phase: EnterBackupPhase, // The context needed to process the result
    // },

    // TODO: Implement these widgets
    // Ready(ReadyScreen),
    // KeyGenPendingFinalize(KeyGenPendingFinalizeScreen),
    // Address(AddressScreen),
    // DisplayBackup(BackupScreen),
}

impl Default for WidgetTree {
    fn default() -> Self {
        WidgetTree::Welcome(Welcome::new())
    }
}
