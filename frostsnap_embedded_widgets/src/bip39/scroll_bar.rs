use crate::palette::PALETTE;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle}
};

pub const SCROLLBAR_WIDTH: u32 = 4;
const MIN_INDICATOR_HEIGHT: u32 = 20;

#[derive(Debug)]
pub struct ScrollBar {
    track_rect: Rectangle,
    last_indicator_rect: Option<Rectangle>,
    content_height: u32,
    viewport_height: u32,
    scroll_position: u32,
}

impl ScrollBar {
    pub fn new(position: Point, height: u32, content_height: u32, viewport_height: u32) -> Self {
        let track_rect = Rectangle::new(
            position,
            Size::new(SCROLLBAR_WIDTH, height)
        );
        
        Self {
            track_rect,
            last_indicator_rect: None,
            content_height,
            viewport_height,
            scroll_position: 0,
        }
    }
    
    pub fn set_scroll_position(&mut self, position: u32) {
        self.scroll_position = position;
    }
    
    pub fn set_content_height(&mut self, content_height: u32) {
        self.content_height = content_height;
    }
    
    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        // Don't draw if content fits in viewport
        if self.content_height <= self.viewport_height {
            return;
        }
        
        // Calculate indicator size and position
        let visible_ratio = self.viewport_height as f32 / self.content_height as f32;
        let indicator_height = ((self.track_rect.size.height as f32 * visible_ratio) as u32)
            .max(MIN_INDICATOR_HEIGHT);
        
        let max_scroll = self.content_height.saturating_sub(self.viewport_height);
        let scroll_ratio = if max_scroll > 0 {
            self.scroll_position as f32 / max_scroll as f32
        } else {
            0.0
        };
        
        let indicator_y = self.track_rect.top_left.y + 
            ((self.track_rect.size.height - indicator_height) as f32 * scroll_ratio) as i32;
        
        let new_indicator_rect = Rectangle::new(
            Point::new(self.track_rect.top_left.x, indicator_y),
            Size::new(SCROLLBAR_WIDTH, indicator_height)
        );
        
        // Only redraw if position changed
        if self.last_indicator_rect != Some(new_indicator_rect) {
            // Clear previous indicator if it exists (with track color)
            if let Some(old_rect) = self.last_indicator_rect {
                let old_indicator = RoundedRectangle::with_equal_corners(
                    old_rect,
                    Size::new(2, 2)
                );
                let _ = old_indicator
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(PALETTE.surface_variant)
                            .build(),
                    )
                    .draw(target);
            }
            
            // Draw scroll track (background) - only on first draw or if track was cleared
            if self.last_indicator_rect.is_none() {
                let track = RoundedRectangle::with_equal_corners(
                    self.track_rect,
                    Size::new(2, 2)
                );
                let _ = track
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(PALETTE.surface_variant)
                            .build(),
                    )
                    .draw(target);
            }
            
            // Draw new indicator
            let indicator = RoundedRectangle::with_equal_corners(
                new_indicator_rect,
                Size::new(2, 2)
            );
            let _ = indicator
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(PALETTE.on_surface_variant)
                        .build(),
                )
                .draw(target);
            
            self.last_indicator_rect = Some(new_indicator_rect);
        }
    }
}