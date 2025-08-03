use super::Widget;
use crate::Instant;
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
    text::{Text as EgText, TextStyle, TextStyleBuilder, Alignment, Baseline, renderer::{CharacterStyle, TextRenderer}},
    primitives::{Line, PrimitiveStyle},
    Drawable,
};

/// Distance in pixels between the bottom of the text and the underline
const UNDERLINE_DISTANCE: i32 = 2;

/// A simple text widget that renders text at a specific position
#[derive(Clone)]
pub struct Text<S: CharacterStyle> {
    text: String,
    character_style: S,
    text_style: TextStyle,
    underline_color: Option<<S as CharacterStyle>::Color>,
    drawn: bool,
}

impl<S: CharacterStyle> Text<S> {
    pub fn new<T: Into<String>>(text: T, character_style: S) -> Self {
        let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
            
        Self {
            text: text.into(),
            character_style,
            text_style,
            underline_color: None,
            drawn: false,
        }
    }
    
    pub fn text(&self) -> &str {
        &self.text
    }
    
    /// Create the EgText object at the given position
    fn create_eg_text(&self) -> EgText<'_, S> {
        EgText::with_text_style(
            &self.text,
            Point::zero(),
            self.character_style.clone(),
            self.text_style,
        )
    }
    
    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.text_style = TextStyleBuilder::from(&self.text_style)
            .alignment(alignment)
            .build();
        self
    }
    
    pub fn with_baseline(mut self, baseline: Baseline) -> Self {
        self.text_style = TextStyleBuilder::from(&self.text_style)
            .baseline(baseline)
            .build();
        self
    }
    
    pub fn with_underline(mut self, color: <S as CharacterStyle>::Color) -> Self {
        self.underline_color = Some(color);
        self
    }
}

impl<S, C> Widget for Text<S>
where
    C: PixelColor,
    S: CharacterStyle<Color = C> + TextRenderer<Color = C> + Clone,
{
    type Color = C;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: Instant,
    ) -> Result<(), D::Error> {
        if !self.drawn {
            let text_obj = self.create_eg_text();
            text_obj.draw(target)?;
            
            // Draw underline if enabled
            if let Some(underline_color) = self.underline_color {
                let text_bbox = text_obj.bounding_box();
                let underline_y = text_bbox.bottom_right().unwrap().y + UNDERLINE_DISTANCE;
                
                Line::new(
                    Point::new(text_bbox.top_left.x, underline_y),
                    Point::new(text_bbox.bottom_right().unwrap().x, underline_y)
                )
                .into_styled(PrimitiveStyle::with_stroke(underline_color, 1))
                .draw(target)?;
            }
            
            self.drawn = true;
        }
        
        Ok(())
    }
    
    fn handle_touch(&mut self, _point: Point, _current_time: Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No drag handling needed
    }
    
    fn size_hint(&self) -> Option<Size> {
        // Create text at origin to get its bounding box
        let text_obj = self.create_eg_text();
        let bbox = text_obj.bounding_box();
        let mut size = bbox.size;
        // If underline is enabled, add space for it
        if self.underline_color.is_some() {
            size.height += UNDERLINE_DISTANCE as u32 + 1; // +1 for the underline stroke itself
        }
        
        Some(size)
    }
    
    fn force_full_redraw(&mut self) {
        self.drawn = false;
    }
}
