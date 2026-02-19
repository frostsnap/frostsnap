use crate::super_draw_target::SuperDrawTarget;
use crate::{palette::PALETTE, Frac, Rat, Widget};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
};

pub const SCROLLBAR_WIDTH: u32 = 4;
const MIN_INDICATOR_HEIGHT: u32 = 20;
const CORNER_RADIUS: Size = Size::new(2, 2);

#[derive(Debug, PartialEq)]
pub struct ScrollBar {
    last_scroll_position: Option<Rat>,
    thumb_size: Frac,
    scroll_position: Rat,
    height: Option<u32>,
}

impl ScrollBar {
    pub fn new(thumb_size: Frac) -> Self {
        Self {
            last_scroll_position: None,
            thumb_size,
            scroll_position: Rat::ZERO,
            height: None,
        }
    }

    fn thumb_rect(&self, track_rect: &Rectangle, scroll_position: Rat) -> Rectangle {
        let thumb_height = (self.thumb_size * track_rect.size.height)
            .max(Rat::from_int(MIN_INDICATOR_HEIGHT as _));
        let available_track_height = track_rect.size.height - thumb_height;
        let thumb_y =
            track_rect.top_left.y + (scroll_position * available_track_height).round() as i32;
        Rectangle::new(
            Point::new(track_rect.top_left.x, thumb_y),
            Size::new(track_rect.size.width, thumb_height.round()),
        )
    }

    pub fn set_scroll_position(&mut self, position: Rat) {
        self.scroll_position = position;
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if self.thumb_size >= Frac::ONE {
            return;
        }

        let bounds = target.bounding_box();
        let track_rect = Rectangle::new(bounds.top_left, bounds.size);
        let new_thumb = self.thumb_rect(&track_rect, self.scroll_position);

        let track_style = PrimitiveStyleBuilder::new()
            .fill_color(PALETTE.surface_variant)
            .build();
        let thumb_style = PrimitiveStyleBuilder::new()
            .fill_color(PALETTE.on_surface_variant)
            .build();

        if let Some(last_pos) = self.last_scroll_position {
            if last_pos == self.scroll_position {
                return;
            }
            let old_thumb = self.thumb_rect(&track_rect, last_pos);
            let displacement = new_thumb.top_left.y - old_thumb.top_left.y;
            // 🔘 extend by corner radius so the old thumb's rounded pixels get cleared
            let clear_height =
                (displacement.unsigned_abs() + CORNER_RADIUS.height).min(old_thumb.size.height);
            if clear_height > 0 {
                let clear_y = if displacement > 0 {
                    old_thumb.top_left.y
                } else {
                    old_thumb.top_left.y + old_thumb.size.height as i32 - clear_height as i32
                };
                let strip = RoundedRectangle::with_equal_corners(
                    Rectangle::new(
                        Point::new(track_rect.top_left.x, clear_y),
                        Size::new(track_rect.size.width, clear_height),
                    ),
                    CORNER_RADIUS,
                );
                let _ = strip.into_styled(track_style).draw(target);
            }
        } else {
            let track = RoundedRectangle::with_equal_corners(track_rect, CORNER_RADIUS);
            let _ = track.into_styled(track_style).draw(target);
        }

        let thumb = RoundedRectangle::with_equal_corners(new_thumb, CORNER_RADIUS);
        let _ = thumb.into_styled(thumb_style).draw(target);

        self.last_scroll_position = Some(self.scroll_position);
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
