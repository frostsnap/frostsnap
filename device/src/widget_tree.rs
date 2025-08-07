use alloc::boxed::Box;
use frostsnap_core::device::{KeyGenPhase2, SignPhase1};
use frostsnap_embedded_widgets::{
    keygen_check::KeygenCheck, sign_prompt::SignPrompt, DeviceNameScreen, Standby, Welcome,
};
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
    DeviceNaming(DeviceNameScreen),

    /// Keygen confirmation screen
    KeygenCheck {
        widget: KeygenCheck,
        phase: Option<Box<KeyGenPhase2>>,
    },

    /// Sign transaction prompt screen
    SignPrompt {
        widget: SignPrompt,
        phase: Option<Box<SignPhase1>>,
    },
    // TODO: Re-enable when EnterShareIndexScreen and EnterShareScreen implement Widget trait
    // /// Complex interactive screens for entering backup
    // EnterBackup {
    //     stage: EnteringBackupStage,
    //     phase: EnterBackupPhase, // The context needed to process the result
    // },

    // TODO: Implement ProgressBar widget or wrap ProgressBars
    // /// Progress indicators
    // FirmwareUpgrade(ProgressBars),

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
