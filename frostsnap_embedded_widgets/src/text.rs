use super::Widget;
use crate::{FONT_SMALL, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Point,
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Baseline, Text as EgText, TextStyleBuilder}
};
use u8g2_fonts::U8g2TextStyle;

/// A simple text widget that renders text centered in the draw target
pub struct Text {
    text: &'static str,
    alignment: Alignment,
    baseline: Baseline,
    drawn: bool,
}

impl Text {
    pub fn new(text: &'static str) -> Self {
        Self {
            text,
            alignment: Alignment::Center,
            baseline: Baseline::Middle,
            drawn: false,
        }
    }
    
    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
    
    pub fn with_baseline(mut self, baseline: Baseline) -> Self {
        self.baseline = baseline;
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
            let center = bounds.center();
            
            let text_style = U8g2TextStyle::new(FONT_SMALL, BinaryColor::On);
            let character_style = TextStyleBuilder::new()
                .alignment(self.alignment)
                .baseline(self.baseline)
                .build();
                
            EgText::with_text_style(self.text, center, text_style, character_style)
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
}