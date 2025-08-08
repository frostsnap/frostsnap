use crate::{Widget, DynWidget, Instant, mut_text::{MutText, mut_text_buffer_size}, palette::PALETTE, color_map::ColorMap, string_buffer::StringBuffer};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Rgb565, RgbColor, BinaryColor},
    text::Alignment,
};
use alloc::collections::VecDeque;
use core::fmt::Write;

// Constants for MutText dimensions - "FPS: 999" is max 8 chars
const FPS_MAX_CHARS: usize = 10;
const FPS_WIDTH: usize = 80;
const FPS_HEIGHT: usize = 20;
const FPS_BUFFER_SIZE: usize = mut_text_buffer_size::<FPS_WIDTH, FPS_HEIGHT>();

type FpsMutText = MutText<u8g2_fonts::U8g2TextStyle<BinaryColor>, FPS_MAX_CHARS, FPS_WIDTH, FPS_HEIGHT, FPS_BUFFER_SIZE>;

/// A widget that displays frames per second using a 2-second moving average
pub struct Fps {
    display: ColorMap<FpsMutText, Rgb565>,
    frame_timestamps: VecDeque<Instant>,
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
            frame_timestamps: VecDeque::new(),
            last_display_update: None,
            current_fps: 0,
        }
    }
}

impl DynWidget for Fps {
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.display.size_hint()
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
        // Add current frame timestamp
        self.frame_timestamps.push_back(current_time);
        
        // Remove timestamps older than 2 seconds
        while let Some(oldest) = self.frame_timestamps.front() {
            if current_time.saturating_duration_since(*oldest) > 2000 {
                self.frame_timestamps.pop_front();
            } else {
                break;
            }
        }
        
        // Update display every 500ms
        let should_update = match self.last_display_update {
            Some(last_update) => current_time.saturating_duration_since(last_update) >= 500,
            None => true,
        };
        
        if should_update {
            // Calculate FPS from timestamps in the 2-second window
            let fps = if self.frame_timestamps.len() >= 2 {
                // Get time span of all frames
                let first = self.frame_timestamps.front().unwrap();
                let last = self.frame_timestamps.back().unwrap();
                let time_span_ms = last.saturating_duration_since(*first);
                
                if time_span_ms > 0 {
                    // Calculate FPS: (frames - 1) * 1000 / time_span_ms
                    // We use frames - 1 because the span is between first and last frame
                    ((self.frame_timestamps.len() - 1) as u64 * 1000) / time_span_ms
                } else {
                    0
                }
            } else {
                0
            };
            
            // Only update display if FPS changed
            if fps != self.current_fps as u64 {
                self.current_fps = fps as u32;
                // Use a small buffer to format the string without allocation
                let mut buf = StringBuffer::<FPS_MAX_CHARS>::new();
                write!(&mut buf, "FPS: {}", self.current_fps).ok();
                self.display.child.set_text(buf.as_str());
            }
            
            self.last_display_update = Some(current_time);
        }
        
        // Draw the display
        self.display.draw(target, current_time)
    }
}
