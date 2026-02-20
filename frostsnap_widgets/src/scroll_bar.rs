use crate::super_draw_target::SuperDrawTarget;
use crate::{palette::PALETTE, Frac, Rat, Widget};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
};

pub const SCROLLBAR_WIDTH: u32 = 4;
const MIN_INDICATOR_HEIGHT: u32 = 20;

#[derive(Debug, PartialEq)]
pub struct ScrollBar {
    last_scroll_position: Option<Rat>,
    thumb_size: Frac,
    scroll_position: Rat,
    height: Option<u32>,
    last_thumb_rect: Option<Rectangle>,
}

impl ScrollBar {
    pub fn new(thumb_size: Frac) -> Self {
        Self {
            last_scroll_position: None,
            thumb_size,
            scroll_position: Rat::ZERO,
            height: None,
            last_thumb_rect: None,
        }
    }

    pub fn set_scroll_position(&mut self, position: Rat) {
        self.scroll_position = position;
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if self.thumb_size >= Frac::ONE {
            // Everything is visible, no need for scrollbar
            return;
        }

        if self.last_scroll_position == Some(self.scroll_position) {
            return;
        }

        let bounds = target.bounding_box();
        let track_rect = Rectangle::new(bounds.top_left, bounds.size);

        let thumb_height = (self.thumb_size * track_rect.size.height)
            .max(Rat::from_int(MIN_INDICATOR_HEIGHT as _));

        let available_track_height = track_rect.size.height - thumb_height;
        let thumb_y =
            track_rect.top_left.y + (self.scroll_position * available_track_height).round() as i32;

        let thumb_rect = Rectangle::new(
            Point::new(track_rect.top_left.x, thumb_y),
            Size::new(track_rect.size.width, thumb_height.round()),
        );

        let track_style = PrimitiveStyleBuilder::new()
            .fill_color(PALETTE.surface_variant)
            .build();
        let thumb_style = PrimitiveStyleBuilder::new()
            .fill_color(PALETTE.on_surface_variant)
            .build();

        if let Some(old_thumb) = self.last_thumb_rect {
            // Only repaint the region vacated by the old thumb, then draw the new thumb.
            // This avoids clearing the entire track which causes flicker.
            let old_top = old_thumb.top_left.y;
            let old_bottom = old_top + old_thumb.size.height as i32;
            let new_top = thumb_rect.top_left.y;
            let new_bottom = new_top + thumb_rect.size.height as i32;

            // Clear the strip of the old thumb that the new thumb doesn't cover
            if new_top > old_top {
                // Old thumb extended above new thumb — clear that strip with track color
                let clear_height = (new_top - old_top).min(old_thumb.size.height as i32);
                let clear = Rectangle::new(
                    Point::new(track_rect.top_left.x, old_top),
                    Size::new(track_rect.size.width, clear_height as u32),
                );
                let _ = clear.into_styled(track_style).draw(target);
            }
            if new_bottom < old_bottom {
                // Old thumb extended below new thumb — clear that strip with track color
                let clear_height = (old_bottom - new_bottom).min(old_thumb.size.height as i32);
                let clear = Rectangle::new(
                    Point::new(track_rect.top_left.x, new_bottom),
                    Size::new(track_rect.size.width, clear_height as u32),
                );
                let _ = clear.into_styled(track_style).draw(target);
            }
        } else {
            // First draw: paint the full track background
            let track = RoundedRectangle::with_equal_corners(track_rect, Size::new(2, 2));
            let _ = track.into_styled(track_style).draw(target);
        }

        // Draw the thumb
        let thumb = RoundedRectangle::with_equal_corners(thumb_rect, Size::new(2, 2));
        let _ = thumb.into_styled(thumb_style).draw(target);

        self.last_scroll_position = Some(self.scroll_position);
        self.last_thumb_rect = Some(thumb_rect);
    }
}

impl crate::DynWidget for ScrollBar {
    fn sizing(&self) -> crate::Sizing {
        crate::Sizing {
            width: SCROLLBAR_WIDTH,
            height: self
                .height
                .expect("ScrollBar::sizing called before set_constraints"),
            ..Default::default()
        }
    }

    fn set_constraints(&mut self, max_size: Size) {
        self.height = Some(max_size.height);
    }

    fn force_full_redraw(&mut self) {
        self.last_scroll_position = None;
        self.last_thumb_rect = None;
    }
}

impl Widget for ScrollBar {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        self.draw(target);
        Ok(())
    }
}
