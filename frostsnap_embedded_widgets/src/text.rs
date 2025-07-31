use super::Widget;
use crate::Instant;
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
    text::{Text as EgText, TextStyle, TextStyleBuilder, Alignment, Baseline, renderer::{CharacterStyle, TextRenderer}},
    Drawable,
};

/// A simple text widget that renders text at a specific position
#[derive(Clone)]
pub struct Text<S> {
    text: String,
    character_style: S,
    text_style: TextStyle,
    drawn: bool,
}

impl<S> Text<S> {
    pub fn new<T: Into<String>>(text: T, character_style: S) -> Self {
        let text_style = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();
            
        Self {
            text: text.into(),
            character_style,
            text_style,
            drawn: false,
        }
    }
    
    pub fn text(&self) -> &str {
        &self.text
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
            let bounds = target.bounding_box();
            let center = bounds.center();
            
            EgText::with_text_style(
                &self.text,
                center,
                self.character_style.clone(),
                self.text_style,
            )
            .draw(target)?;
            
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
        // Use Dimensions trait to get the actual text dimensions
        let text = EgText::with_text_style(
            &self.text,
            Point::zero(),
            self.character_style.clone(),
            self.text_style,
        );
        
        // Get the bounding box dimensions
        let bbox = text.bounding_box();
        Some(bbox.size)
    }
    
    fn force_full_redraw(&mut self) {
        self.drawn = false;
    }
}
