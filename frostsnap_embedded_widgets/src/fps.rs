use crate::{Widget, DynWidget, Instant, mut_text::{MutText, mut_text_buffer_size}, palette::PALETTE, color_map::ColorMap, string_buffer::StringBuffer};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Rgb565, RgbColor, BinaryColor},
    text::Alignment,
};
use core::fmt::Write;

// Constants for MutText dimensions - "FPS: 999" is max 8 chars
const FPS_MAX_CHARS: usize = 10;
// ProFont17 is exactly 10px wide per character for ASCII
// "FPS: 999" = 8 chars = 80px wide
const FPS_WIDTH: usize = 80;  // Exact width for "FPS: 999"
const FPS_HEIGHT: usize = 17;  // ProFont17 is exactly 17px tall
const FPS_BUFFER_SIZE: usize = mut_text_buffer_size::<FPS_WIDTH, FPS_HEIGHT>();

type FpsMutText = MutText<u8g2_fonts::U8g2TextStyle<BinaryColor>, FPS_MAX_CHARS, FPS_WIDTH, FPS_HEIGHT, FPS_BUFFER_SIZE>;

/// A widget that displays frames per second using simple frame counting
pub struct Fps {
    display: ColorMap<FpsMutText, Rgb565>,
    frame_count: u32,
    last_fps_time: Option<Instant>,
    last_display_update: Option<Instant>,
    current_fps: u32,
}

impl Fps {
    /// Create a new FPS counter widget with green text
    pub fn new() -> Self {
        let text_style = u8g2_fonts::U8g2TextStyle::new(
            crate::FONT_SMALL,
            BinaryColor::On,
        );
        let mut_text = MutText::new(
            "FPS: 0",
            text_style,
        ).with_alignment(Alignment::Left);
        
        let display = mut_text.color_map(|c| match c {
            BinaryColor::On => Rgb565::GREEN,
            BinaryColor::Off => PALETTE.background,
        });
        
        Self {
            display,
            frame_count: 0,
            last_fps_time: None,
            last_display_update: None,
            current_fps: 0,
        }
    }
}

impl DynWidget for Fps {
    fn set_constraints(&mut self, max_size: Size) {
        self.display.set_constraints(max_size);
    }
    
    fn sizing(&self) -> crate::Sizing {
        self.display.sizing()
    }
    
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }
    
    
    fn force_full_redraw(&mut self) {
        self.display.force_full_redraw();
    }
}

impl Widget for Fps {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Count frames
        self.frame_count += 1;
        
        // Calculate FPS every second
        let should_calculate = match self.last_fps_time {
            Some(last_time) => {
                let elapsed = current_time.saturating_duration_since(last_time);
                elapsed >= 1000
            },
            None => true,
        };
        
        if should_calculate {
            if let Some(last_time) = self.last_fps_time {
                let elapsed_ms = current_time.saturating_duration_since(last_time);
                if elapsed_ms > 0 {
                    // Calculate FPS: frames * 1000 / elapsed_ms
                    let fps = (self.frame_count as u64 * 1000) / elapsed_ms;
                    self.current_fps = fps as u32;
                }
            }
            
            // Reset counter for next second
            self.frame_count = 0;
            self.last_fps_time = Some(current_time);
        }
        
        // Update display every 500ms
        let should_update_display = match self.last_display_update {
            Some(last_update) => current_time.saturating_duration_since(last_update) >= 500,
            None => true,
        };
        
        if should_update_display {
            // Format and update the display
            let mut buf = StringBuffer::<FPS_MAX_CHARS>::new();
            write!(&mut buf, "FPS: {}", self.current_fps).ok();
            self.display.child.set_text(buf.as_str());
            
            self.last_display_update = Some(current_time);
        }
        
        // Draw the display
        self.display.draw(target, current_time)
    }
}
