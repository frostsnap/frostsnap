use super::Widget;
use crate::Instant;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Point,
    pixelcolor::BinaryColor,
    prelude::*,
};
use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::TextBoxStyleBuilder,
    TextBox,
};
use u8g2_fonts::U8g2TextStyle;

// Re-export for legacy compatibility
pub use embedded_graphics::text::Baseline;

/// A simple text widget that renders text in a bounded area
pub struct Text {
    text: &'static str,
    horizontal_alignment: HorizontalAlignment,
    vertical_alignment: VerticalAlignment,
    drawn: bool,
}

impl Text {
    pub fn new(text: &'static str) -> Self {
        Self {
            text,
            horizontal_alignment: HorizontalAlignment::Center,
            vertical_alignment: VerticalAlignment::Middle,
            drawn: false,
        }
    }
}

impl Text {
    pub fn with_horizontal_alignment(mut self, alignment: HorizontalAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }
    
    pub fn with_vertical_alignment(mut self, alignment: VerticalAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }
    
    // Legacy compatibility methods
    pub fn with_baseline(mut self, baseline: embedded_graphics::text::Baseline) -> Self {
        use embedded_graphics::text::Baseline;
        self.vertical_alignment = match baseline {
            Baseline::Top => VerticalAlignment::Top,
            Baseline::Middle => VerticalAlignment::Middle,
            Baseline::Bottom => VerticalAlignment::Bottom,
            _ => VerticalAlignment::Middle,
        };
        self
    }
}

impl Widget for Text {
    type Color = BinaryColor;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: Instant,
    ) -> Result<(), D::Error> {
        if !self.drawn {
            let bounds = target.bounding_box();
            
            // Use FONT_MED for bigger, nicer text
            let character_style = U8g2TextStyle::new(crate::FONT_MED, BinaryColor::On);
            let textbox_style = TextBoxStyleBuilder::new()
                .alignment(self.horizontal_alignment)
                .vertical_alignment(self.vertical_alignment)
                .build();
                
            TextBox::with_textbox_style(
                self.text,
                bounds,
                character_style,
                textbox_style,
            )
            .draw(target)?;
            
            self.drawn = true;
        }
        
        Ok(())
    }
    
    fn handle_touch(&mut self, _point: Point, _current_time: Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _start_y: Option<u32>, _current_y: u32) {
        // Text doesn't respond to drags
    }
    
    fn size_hint(&self) -> Option<Size> {
        None
    }
    
    fn force_full_redraw(&mut self) {
        self.drawn = false;
    }
}