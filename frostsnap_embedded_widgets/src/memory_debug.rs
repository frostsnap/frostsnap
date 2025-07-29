use embedded_graphics::{
    mono_font::{ascii, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Text}
};
use super::Widget;

#[derive(Debug)]
pub struct MemoryDebugWidget {
    last_used: usize,
    last_free: usize,
    buffer: [u8; 16], // "255k/255k" worst case
    len: usize,
    position: Point,
    background_rect: Rectangle,
    last_draw_time: Option<crate::Instant>,
}

impl MemoryDebugWidget {
    pub fn new(screen_width: u32, _screen_height: u32) -> Self {
        // Position at top center
        let y = 20; // Near the top
        let x = (screen_width / 2) as i32;
        
        // Background rectangle centered around the text
        let bg_width = 80;
        let bg_rect = Rectangle::new(
            Point::new(x - (bg_width / 2) as i32, y - 10),
            Size::new(bg_width, 18),
        );
        
        Self {
            last_used: 0,
            last_free: 0,
            buffer: [0; 16],
            len: 0,
            position: Point::new(x, y), // Center point for text
            background_rect: bg_rect,
            last_draw_time: None,
        }
    }
    
    pub fn update(&mut self, used: usize, free: usize) -> bool {
        if used == self.last_used && free == self.last_free {
            return false; // No change, no need to redraw
        }
        
        self.last_used = used;
        self.last_free = free;
        
        // Format the string into our buffer
        let used_kb = used / 1024;
        let total_kb = (used + free) / 1024;
        
        // Format manually to avoid allocations
        self.len = 0;
        
        // Format used_kb
        self.format_number(used_kb);
        
        // Add 'k/'
        self.buffer[self.len] = b'k';
        self.len += 1;
        self.buffer[self.len] = b'/';
        self.len += 1;
        
        // Format total_kb
        self.format_number(total_kb);
        
        // Add 'k'
        self.buffer[self.len] = b'k';
        self.len += 1;
        
        true // Values changed, need to redraw
    }
    
    fn format_number(&mut self, mut num: usize) {
        if num == 0 {
            self.buffer[self.len] = b'0';
            self.len += 1;
            return;
        }
        
        // Get digits in reverse order
        let start = self.len;
        while num > 0 {
            self.buffer[self.len] = b'0' + (num % 10) as u8;
            self.len += 1;
            num /= 10;
        }
        
        // Reverse the digits
        let end = self.len - 1;
        let mut i = start;
        let mut j = end;
        while i < j {
            self.buffer.swap(i, j);
            i += 1;
            j -= 1;
        }
    }
    
}

impl Widget for MemoryDebugWidget {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Check if we should redraw (every 1 second)
        if let Some(last_time) = self.last_draw_time {
            let elapsed_ms = current_time.saturating_duration_since(last_time);
            if elapsed_ms < 1000 {
                return Ok(()); // Don't redraw yet
            }
        }
        
        self.last_draw_time = Some(current_time);
        // Clear the background
        self.background_rect
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(target)?;
        
        // Draw the text
        if self.len > 0 {
            let text = core::str::from_utf8(&self.buffer[..self.len])
                .unwrap_or("ERR");
            
            Text::with_alignment(
                text,
                self.position,
                MonoTextStyle::new(&ascii::FONT_6X10, Rgb565::GREEN),
                Alignment::Center,
            )
            .draw(target)?;
        }
        
        Ok(())
    }
}