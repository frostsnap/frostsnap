use crate::{Widget, string_buffer::StringBuffer};
use embedded_graphics::{
    draw_target::DrawTarget,
    framebuffer::{Framebuffer, buffer_size},
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, raw::{RawU1, LittleEndian}},
    prelude::*,
    text::{Text as EgText, TextStyle, TextStyleBuilder, Alignment, Baseline, renderer::{CharacterStyle, TextRenderer}},
};

/// A mutable text widget with a fixed-size framebuffer
/// N = max number of bytes for the text string
/// W = width in pixels
/// H = height in pixels
/// BUFFER_SIZE = buffer size in bytes (must be calculated externally)
pub struct MutText<S, const N: usize, const W: usize, const H: usize, const BUFFER_SIZE: usize> 
where
    S: CharacterStyle<Color = BinaryColor> + TextRenderer<Color = BinaryColor> + Clone,
{
    text: StringBuffer<N>,
    character_style: S,
    text_style: TextStyle,
    buffer: Framebuffer<BinaryColor, RawU1, LittleEndian, W, H, BUFFER_SIZE>,
    dirty: bool,
}

impl<S, const N: usize, const W: usize, const H: usize, const BUFFER_SIZE: usize> MutText<S, N, W, H, BUFFER_SIZE>
where
    S: CharacterStyle<Color = BinaryColor> + TextRenderer<Color = BinaryColor> + Clone,
{
    pub fn new(text: &str, character_style: S) -> Self {
        let mut text_buf = StringBuffer::new();
        use core::fmt::Write;
        write!(&mut text_buf, "{}", text).ok();
        
        let text_style = TextStyleBuilder::new()
            .baseline(Baseline::Top)
            .alignment(Alignment::Center)
            .build();
        
        let mut widget = Self {
            text: text_buf,
            character_style,
            text_style,
            buffer: Framebuffer::new(),
            dirty: true,
        };
        
        // Initial render
        widget.render_to_buffer();
        widget
    }
    
    /// Set new text and mark as dirty
    pub fn set_text(&mut self, text: &str) {
        let old_text = self.text.as_str();
        
        // Check if text has changed
        if old_text != text {
            self.text.clear();
            use core::fmt::Write;
            write!(&mut self.text, "{}", text).ok();
            self.render_to_buffer();
            self.dirty = true;
        }
    }
    
    /// Get the current text
    pub fn text(&self) -> &str {
        self.text.as_str()
    }
    
    /// Render text to the internal buffer
    fn render_to_buffer(&mut self) {
        // Clear the buffer
        self.buffer.clear(BinaryColor::Off).ok();
        
        // Draw the text
        let text_obj = EgText::with_text_style(
            self.text.as_str(),
            Point::new(W as i32 / 2, 0),
            self.character_style.clone(),
            self.text_style,
        );
        
        text_obj.draw(&mut self.buffer).ok();
    }
    
    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.text_style = TextStyleBuilder::from(&self.text_style)
            .alignment(alignment)
            .build();
        self.render_to_buffer();
        self
    }
    
    pub fn with_baseline(mut self, baseline: Baseline) -> Self {
        self.text_style = TextStyleBuilder::from(&self.text_style)
            .baseline(baseline)
            .build();
        self.render_to_buffer();
        self
    }
}

impl<S, const N: usize, const W: usize, const H: usize, const BUFFER_SIZE: usize> crate::DynWidget for MutText<S, N, W, H, BUFFER_SIZE>
where
    S: CharacterStyle<Color = BinaryColor> + TextRenderer<Color = BinaryColor> + Clone,

{
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No drag handling needed
    }

    fn size_hint(&self) -> Option<Size> {
        Some(Size::new(W as u32, H as u32))
    }

    fn force_full_redraw(&mut self) {
        self.dirty = true;
    }
}

impl<S, const N: usize, const W: usize, const H: usize, const BUFFER_SIZE: usize> Widget for MutText<S, N, W, H, BUFFER_SIZE>
where
    S: CharacterStyle<Color = BinaryColor> + TextRenderer<Color = BinaryColor> + Clone,
{
    type Color = BinaryColor;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.dirty {
            // Draw the framebuffer as an image
            self.buffer.as_image().draw(target)?;
            self.dirty = false;
        }
        
        Ok(())
    }
    
}

/// Helper to calculate buffer size at compile time
pub const fn mut_text_buffer_size<const W: usize, const H: usize>() -> usize {
    buffer_size::<BinaryColor>(W, H)
}
