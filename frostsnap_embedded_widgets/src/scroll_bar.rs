use crate::{palette::PALETTE, Rat, Widget};
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
    last_scroll_position: Option<Rat>,
    content_height: u32,
    viewport_height: u32,
    scroll_position: Rat,
}

impl ScrollBar {
    pub fn new(position: Point, height: u32, content_height: u32, viewport_height: u32) -> Self {
        let track_rect = Rectangle::new(
            position,
            Size::new(SCROLLBAR_WIDTH, height)
        );
        
        Self {
            track_rect,
            last_scroll_position: None,
            content_height,
            viewport_height,
            scroll_position: Rat::ZERO,
        }
    }
    
    pub fn set_scroll_position(&mut self, position: Rat) {
        self.scroll_position = position;
    }
    
    pub fn set_content_height(&mut self, content_height: u32) {
        self.content_height = content_height;
    }
    
    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if self.content_height <= self.viewport_height {
            return;
        }
        
        if self.last_scroll_position == Some(self.scroll_position) {
            return;
        }
        
        let visible_ratio = Rat::from_ratio(self.viewport_height, self.content_height);
        let indicator_height = (visible_ratio * self.track_rect.size.height).max(Rat::from_int(MIN_INDICATOR_HEIGHT as _));
        
        let available_track_height = self.track_rect.size.height - indicator_height;
        let indicator_y = self.track_rect.top_left.y + (self.scroll_position * available_track_height).round() as i32;
        
        let indicator_rect = Rectangle::new(
            Point::new(self.track_rect.top_left.x, indicator_y),
            Size::new(SCROLLBAR_WIDTH, indicator_height.round())
        );
        
        if self.last_scroll_position.is_none() {
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
        } else {
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
        
        let indicator = RoundedRectangle::with_equal_corners(
            indicator_rect,
            Size::new(2, 2)
        );
        let _ = indicator
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(PALETTE.on_surface_variant)
                    .build(),
            )
            .draw(target);
        
        self.last_scroll_position = Some(self.scroll_position);
    }
}

impl crate::DynWidget for ScrollBar {
    fn size_hint(&self) -> Option<Size> {
        Some(self.track_rect.size)
    }
    
    fn force_full_redraw(&mut self) {
        self.last_scroll_position = None;
    }
}

impl Widget for ScrollBar {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        self.draw(target);
        Ok(())
    }
}
