use crate::layout::Container;
use crate::super_draw_target::SuperDrawTarget;
use crate::translate::Translate;
use crate::{palette::PALETTE, Frac, Rat, Widget};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

pub const SCROLLBAR_WIDTH: u32 = 4;
const MIN_INDICATOR_HEIGHT: u32 = 20;
const CORNER_RADIUS: Size = Size::new(2, 2);

pub struct ScrollBar {
    thumb_size: Frac,
    scroll_position: Rat,
    height: Option<u32>,
    thumb: Option<Translate<Container<()>>>,
    track_drawn: bool,
}

impl ScrollBar {
    pub fn new(thumb_size: Frac) -> Self {
        Self {
            thumb_size,
            scroll_position: Rat::ZERO,
            height: None,
            thumb: None,
            track_drawn: false,
        }
    }

    pub fn set_scroll_position(&mut self, position: Rat) {
        if self.scroll_position == position {
            return;
        }
        self.scroll_position = position;
        let y = self.thumb_y();
        if let Some(ref mut thumb) = self.thumb {
            thumb.animate_to(Point::new(0, y), 0);
        }
    }

    fn thumb_height(&self) -> u32 {
        let height = self.height.unwrap_or(0);
        (self.thumb_size * height)
            .max(Rat::from_int(MIN_INDICATOR_HEIGHT as _))
            .round()
    }

    fn thumb_y(&self) -> i32 {
        let height = self.height.unwrap_or(0);
        let thumb_height = Rat::from_int(self.thumb_height() as _);
        let available = height - thumb_height;
        (self.scroll_position * available).round() as i32
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

        let thumb_height = self.thumb_height();
        let mut container = Container::with_size((), Size::new(SCROLLBAR_WIDTH, thumb_height))
            .with_fill(PALETTE.on_surface_variant)
            .with_corner_radius(CORNER_RADIUS);
        container.set_constraints(Size::new(SCROLLBAR_WIDTH, thumb_height));

        let mut translate = Translate::new(container, PALETTE.surface_variant);
        translate.set_constraints(max_size);
        translate.set_offset(Point::new(0, self.thumb_y()));
        self.thumb = Some(translate);
    }

    fn force_full_redraw(&mut self) {
        self.track_drawn = false;
        if let Some(ref mut thumb) = self.thumb {
            thumb.force_full_redraw();
        }
    }
}

impl Widget for ScrollBar {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.thumb_size >= Frac::ONE {
            return Ok(());
        }

        if !self.track_drawn {
            let bounds = target.bounding_box();
            let track_style = embedded_graphics::primitives::PrimitiveStyleBuilder::new()
                .fill_color(PALETTE.surface_variant)
                .build();
            let track = embedded_graphics::primitives::RoundedRectangle::with_equal_corners(
                bounds,
                CORNER_RADIUS,
            );
            let _ = track.into_styled(track_style).draw(target);
            self.track_drawn = true;
        }

        if let Some(ref mut thumb) = self.thumb {
            thumb.draw(target, current_time)?;
        }

        Ok(())
    }
}
