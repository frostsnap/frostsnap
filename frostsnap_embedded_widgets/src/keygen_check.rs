use super::{Widget, Text, HoldToConfirm, Container, Column, Padding, Row};
use crate::{Instant, palette::PALETTE, column::MainAxisAlignment};
use alloc::format;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    primitives::{PrimitiveStyleBuilder, StrokeAlignment},
};
use u8g2_fonts::U8g2TextStyle;

type CodeText = Text<U8g2TextStyle<Rgb565>>;
type TofNText = Text<U8g2TextStyle<Rgb565>>;
type CodeColumn = Column<(TofNText, CodeText), Rgb565>;
type PaddedCodeColumn = Padding<CodeColumn>;
type CodeContainer = Container<PaddedCodeColumn>;
type ConfirmText = Text<U8g2TextStyle<Rgb565>>;
type OnAllDevicesRow = Row<(ConfirmText, ConfirmText), Rgb565>;
type PromptColumn = Column<(ConfirmText, CodeContainer, OnAllDevicesRow), Rgb565>;
type SuccessColumn = Column<(ConfirmText, CodeContainer, OnAllDevicesRow), Rgb565>;

/// Widget for checking and confirming key generation
pub struct KeygenCheck {
    /// The hold-to-confirm widget
    hold_to_confirm: HoldToConfirm<PromptColumn, SuccessColumn>,
}

impl KeygenCheck {
    /// Create a new keygen check widget
    pub fn new(security_check_code: [u8; 4], t_of_n: (u16, u16)) -> Self {
        // Format the t of n string
        let t_of_n_text = format!("{} of {}", t_of_n.0, t_of_n.1);
        
        // Format the security check code as hex
        let hex_code = format!("{:02x}{:02x} {:02x}{:02x}",
            security_check_code[0], 
            security_check_code[1], 
            security_check_code[2], 
            security_check_code[3]
        );
        
        // Create the t of n text widget
        let t_of_n_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_surface);
        let t_of_n_widget = Text::new(t_of_n_text.clone(), t_of_n_style.clone());
        
        // Create the hex code text widget using CODE_FONT
        let code_style = U8g2TextStyle::new(crate::CODE_FONT, PALETTE.on_surface);
        let code_widget = Text::new(hex_code.clone(), code_style);
        
        // Create internal column with t_of_n and code
        let code_column = Column::new((t_of_n_widget, code_widget));

        // Put the column in a container with a border
        let border_style = PrimitiveStyleBuilder::new()
            .stroke_color(PALETTE.outline)
            .stroke_width(2)
            .fill_color(PALETTE.surface)
            .stroke_alignment(StrokeAlignment::Inside)
            .build();
        
        let padded_code_column = Padding::all(10, code_column);
        let code_container = Container::new(padded_code_column)
            .with_border(border_style)
            .with_corner_radius(Size::new(8, 8));
        
        // Create the "confirm identical" text
        let confirm_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_background);
        let confirm_identical_widget = Text::new("Confirm identical:", confirm_style.clone());
        
        // Create the "on all devices" text with underline
        let on_text = Text::new("on ", confirm_style.clone());
        let all_devices_text = Text::new("all devices", confirm_style)
            .with_underline(PALETTE.on_background);
        
        // Clone widgets for success state before moving them
        let success_confirm_identical = confirm_identical_widget.clone();
        let success_on_text = on_text.clone();
        let success_all_devices_text = all_devices_text.clone();
        
        let on_all_devices_row = Row::new((on_text, all_devices_text));
        
        // Create the prompt column with SpaceEvenly alignment
        let prompt_column = Column::new((confirm_identical_widget, code_container, on_all_devices_row))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        
        // Create success widget - clone the prompt structure with tertiary colors
        let success_on_all_devices_row = Row::new((success_on_text, success_all_devices_text));
        
        // Create success code container with tertiary colors
        let success_t_of_n_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_tertiary_container);
        let success_t_of_n_widget = Text::new(t_of_n_text, success_t_of_n_style);
        
        let success_code_style = U8g2TextStyle::new(crate::CODE_FONT, PALETTE.on_tertiary_container);
        let success_code_widget = Text::new(hex_code, success_code_style);
        
        let success_code_column = Column::new((success_t_of_n_widget, success_code_widget));
        let success_padded_code_column = Padding::all(10, success_code_column);
        
        // Create border style with tertiary container colors
        let success_border_style = PrimitiveStyleBuilder::new()
            .stroke_color(PALETTE.outline)
            .stroke_width(1)
            .fill_color(PALETTE.tertiary_container)
            .stroke_alignment(StrokeAlignment::Inside)
            .build();
        
        let success_code_container = Container::new(success_padded_code_column)
            .with_border(success_border_style)
            .with_corner_radius(Size::new(8, 8));
        
        // Create success column with same structure as prompt
        let success_column = Column::new((success_confirm_identical, success_code_container, success_on_all_devices_row))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        
        // Create the hold-to-confirm widget
        let hold_to_confirm = HoldToConfirm::new(
            Size::new(240, 280),
            2000, // 2 second hold duration
            prompt_column,
            success_column,
        );
        
        Self {
            hold_to_confirm,
        }
    }
    
    /// Check if the user has confirmed
    pub fn is_confirmed(&self) -> bool {
        self.hold_to_confirm.is_completed()
    }
    
    /// Reset the confirmation state
    pub fn reset(&mut self) {
        self.hold_to_confirm.reset();
    }
}

impl Widget for KeygenCheck {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.hold_to_confirm.draw(target, current_time)
    }
    
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.hold_to_confirm.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.hold_to_confirm.handle_vertical_drag(prev_y, new_y, is_release)
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.hold_to_confirm.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.hold_to_confirm.force_full_redraw()
    }
}

