use super::{Widget, Text, HoldToConfirm, Container, Column, Padding};
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
type PaddedCodeText = Padding<CodeText>;
type CodeContainer = Container<PaddedCodeText>;
type ConfirmText = Text<U8g2TextStyle<Rgb565>>;
type PromptColumn = Column<(CodeContainer, ConfirmText), Rgb565>;
type SuccessColumn = Column<(CodeContainer, ConfirmText), Rgb565>;

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
        let t_of_n_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_secondary);
        let _t_of_n_widget = Text::new(t_of_n_text.clone(), t_of_n_style.clone());
        
        // Create the hex code text widget using CODE_FONT
        let code_style = U8g2TextStyle::new(crate::CODE_FONT, PALETTE.on_primary_container);
        let code_widget = Text::new(hex_code.clone(), code_style);
        
        // Put the code in a container with a border
        let border_style = PrimitiveStyleBuilder::new()
            .stroke_color(PALETTE.outline)
            .stroke_width(1)
            .fill_color(PALETTE.primary_container)
            .stroke_alignment(StrokeAlignment::Inside)
            .build();
        
        let mut padded_code = Padding::all(15, code_widget);
        // HACK: there is too much space at the bottom because of how the font works.
        padded_code.bottom -= 5;
        let code_container = Container::new(padded_code)
            .with_border(border_style)
            .with_corner_radius(Size::new(8, 8));
        
        // Create the "hold to confirm" text
        let confirm_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_background);
        let confirm_widget = Text::new("hold to confirm", confirm_style.clone());
        
        // Create the prompt column with SpaceEvenly alignment
        let prompt_column = Column::new((code_container, confirm_widget))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        
        // Create success widgets (recreate them)
        let _success_t_of_n_widget = Text::new(t_of_n_text, t_of_n_style);
        
        let success_code_style = U8g2TextStyle::new(crate::CODE_FONT, PALETTE.on_primary_container);
        let success_code_widget = Text::new(hex_code, success_code_style);
        let success_padded_code = Padding::all(15, success_code_widget);
        let success_code_container = Container::new(success_padded_code)
            .with_border(border_style)
            .with_corner_radius(Size::new(8, 8));
        
        let success_confirm_widget = Text::new("hold to confirm", confirm_style);
        
        let success_column = Column::new((success_code_container, success_confirm_widget))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        
        // Create the hold-to-confirm widget
        let hold_to_confirm = HoldToConfirm::new(
            Size::new(240, 280),
            2000.0, // 2 second hold duration
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

