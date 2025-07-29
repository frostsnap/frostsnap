use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::*,
};
use frostsnap_embedded_widgets::{
    Widget, Welcome,
    device_name::DeviceName,
};
use crate::ui::UiEvent;
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
pub enum WidgetTree {
    /// Default welcome screen
    Welcome(Welcome),
    
    // TODO: Re-enable when Text widget is properly implemented
    // /// Simple text display
    // Text(TextWidget),
    
    // /// Interactive prompts with hold-to-confirm
    // Prompt {
    //     widget: HoldToConfirm<TextWidget, TextWidget>,
    //     data: Prompt, // The associated prompt data
    // },
    
    /// Device naming screen
    DeviceNaming(DeviceName),
    
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

impl WidgetTree {
    /// Handle touch input and return any resulting UI event
    pub fn handle_touch(&mut self, _point: Point, _current_time: frostsnap_embedded_widgets::Instant, _is_release: bool) -> Option<UiEvent> {
        match self {
            // WidgetTree::Prompt { widget, data } => {
            //     widget.handle_touch(point, current_time, is_release);
            //     if widget.is_completed() {
            //         // Return the appropriate event based on prompt type
            //         Some(match data {
            //             Prompt::KeyGen { phase } => UiEvent::KeyGenConfirm { phase: phase.clone() },
            //             Prompt::Signing { phase } => UiEvent::SigningConfirm { phase: phase.clone() },
            //             Prompt::NewName { new_name, .. } => UiEvent::NameConfirm(new_name.clone()),
            //             Prompt::DisplayBackupRequest { phase } => UiEvent::BackupRequestConfirm { phase: phase.clone() },
            //             Prompt::ConfirmFirmwareUpgrade { .. } => UiEvent::UpgradeConfirm,
            //             Prompt::ConfirmEnterBackup { phase, share_backup } => UiEvent::EnteredShareBackup {
            //                 phase: phase.clone(),
            //                 share_backup: *share_backup,
            //             },
            //             Prompt::WipeDevice => UiEvent::WipeDataConfirm,
            //         })
            //     } else {
            //         None
            //     }
            // }
            // WidgetTree::EnterBackup { stage, phase } => {
            //     // TODO: Handle backup entry touch events
            //     None
            // }
            _ => None,
        }
    }
    
    /// Handle vertical drag events
    pub fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        match self {
            // WidgetTree::EnterBackup { stage, .. } => {
            //     match stage {
            //         EnteringBackupStage::Share(screen) => {
            //             screen.handle_vertical_drag(prev_y, new_y, is_release);
            //         }
            //         _ => {}
            //     }
            // }
            _ => {}
        }
    }
    
    /// Force a full redraw of the current widget
    pub fn force_redraw(&mut self) {
        match self {
            WidgetTree::Welcome(widget) => widget.force_full_redraw(),
            // WidgetTree::Text(widget) => widget.force_full_redraw(),
            // WidgetTree::Prompt { widget, .. } => widget.force_full_redraw(),
            WidgetTree::DeviceNaming(widget) => widget.force_full_redraw(),
            // WidgetTree::FirmwareUpgrade(widget) => widget.force_full_redraw(),
            // WidgetTree::EnterBackup { stage, .. } => {
            //     match stage {
            //         EnteringBackupStage::ShareIndex(screen) => screen.force_full_redraw(),
            //         EnteringBackupStage::Share(screen) => screen.force_full_redraw(),
            //     }
            // }
        }
    }
}

impl Widget for WidgetTree {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: frostsnap_embedded_widgets::Instant,
    ) -> Result<(), D::Error> {
        // Draw the appropriate widget
        match self {
            WidgetTree::Welcome(widget) => widget.draw(target, current_time)?,
            // WidgetTree::Text(widget) => widget.draw(target, current_time)?,
            // WidgetTree::Prompt { widget, .. } => widget.draw(target, current_time)?,
            WidgetTree::DeviceNaming(widget) => widget.draw(target, current_time)?,
            // WidgetTree::FirmwareUpgrade(widget) => widget.draw(target, current_time)?,
            // WidgetTree::EnterBackup { stage, .. } => {
            //     match stage {
            //         EnteringBackupStage::ShareIndex(screen) => screen.draw(target, current_time)?,
            //         EnteringBackupStage::Share(screen) => screen.draw(target, current_time)?,
            //     }
            // }
        }
        
        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: frostsnap_embedded_widgets::Instant,
        is_release: bool,
    ) -> Option<frostsnap_embedded_widgets::KeyTouch> {
        // WidgetTree handles touch through its own method that returns UiEvent
        self.handle_touch(point, current_time, is_release);
        None
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // Delegate to WidgetTree's own method
        WidgetTree::handle_vertical_drag(self, prev_y, new_y, is_release);
    }
    
    fn size_hint(&self) -> Option<Size> {
        // Full screen for all widgets
        Some(Size::new(240, 280))
    }
    
    fn force_full_redraw(&mut self) {
        WidgetTree::force_redraw(self);
    }
}