use super::{rat::Frac, DynWidget, Widget};
use crate::aa::rounded_rect::AARoundedRectIter;
use crate::fader::FadingDrawTarget;
use crate::super_draw_target::SuperDrawTarget;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{PixelColor, Rgb565},
    prelude::*,
    primitives::Rectangle,
};

const CORNER_RADIUS: u32 = 42;

#[derive(Debug, Clone, PartialEq)]
pub struct HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    pub child: W,
    progress: Frac,
    last_drawn_progress: Frac,
    screen_size: Option<Size>,
    sizing: crate::Sizing,
    border_width: u32,
    border_color: C,
    background_color: C,
    fade_progress: Frac,
    fade_start_time: Option<crate::Instant>,
    fade_duration_ms: u64,
    is_fading: bool,
}

impl<W, C> HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    pub fn new(child: W, border_width: u32, border_color: C, background_color: C) -> Self {
        Self {
            child,
            progress: Frac::ZERO,
            last_drawn_progress: Frac::ZERO,
            screen_size: None,
            sizing: crate::Sizing {
                width: 0,
                height: 0,
                ..Default::default()
            },
            border_width,
            border_color,
            background_color,
            fade_progress: Frac::ZERO,
            fade_start_time: None,
            fade_duration_ms: 0,
            is_fading: false,
        }
    }

    pub fn set_progress(&mut self, progress: Frac) {
        self.progress = progress;
    }

    pub fn get_progress(&self) -> Frac {
        self.progress
    }

    pub fn border_width(&self) -> u32 {
        self.border_width
    }

    pub fn start_fade_out(&mut self, duration_ms: u64) {
        self.is_fading = true;
        self.fade_duration_ms = duration_ms;
        self.fade_start_time = None;
        self.fade_progress = Frac::ZERO;
    }

    pub fn is_fading(&self) -> bool {
        self.is_fading
    }

    pub fn is_faded_out(&self) -> bool {
        self.is_fading && self.fade_progress == Frac::ONE
    }

    pub fn set_border_color(&mut self, color: C) {
        self.border_color = color;
        self.force_full_redraw();
    }

    pub fn set_background_color(&mut self, color: C) {
        self.background_color = color;
        self.force_full_redraw();
    }
}

impl<W, C> crate::DynWidget for HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.screen_size = Some(max_size);

        let child_max_size = Size::new(
            max_size.width.saturating_sub(2 * self.border_width),
            max_size.height.saturating_sub(2 * self.border_width),
        );
        self.child.set_constraints(child_max_size);

        let child_sizing = self.child.sizing();
        self.sizing = crate::Sizing {
            width: child_sizing.width + 2 * self.border_width,
            height: child_sizing.height + 2 * self.border_width,
            ..Default::default()
        };
    }

    fn sizing(&self) -> crate::Sizing {
        self.sizing
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<super::KeyTouch> {
        self.child.handle_touch(point, current_time, lift_up)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, _is_release);
    }

    fn force_full_redraw(&mut self) {
        self.last_drawn_progress = Frac::ZERO;
        self.child.force_full_redraw();
    }
}

/// Interpolate a Frac position between two perimeter points.
fn lerp_frac(a: Frac, b: Frac, t: Frac) -> Frac {
    let span = Frac::new(b.as_rat() - a.as_rat());
    Frac::new(a.as_rat() + (t * span).as_rat())
}

impl<W> Widget for HoldToConfirmBorder<W, Rgb565>
where
    W: Widget<Color = Rgb565>,
{
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let offset = Point::new(self.border_width as i32, self.border_width as i32);
        let child_size = self.child.sizing();
        let child_area = Rectangle::new(offset, Size::new(child_size.width, child_size.height));
        let mut cropped = target.clone().crop(child_area);
        self.child.draw(&mut cropped, current_time)?;

        if self.is_faded_out() {
            return Ok(());
        }

        let size = self.screen_size.unwrap();
        let make_iter = |color: Rgb565| -> AARoundedRectIter<Rgb565> {
            let bg = self.background_color;
            AARoundedRectIter::new(
                size.width,
                size.height,
                CORNER_RADIUS,
                self.border_width,
                color,
                bg,
                bg,
            )
        };

        let w = size.width as i32;
        let mirror =
            move |Pixel(p, c): Pixel<Rgb565>| [Pixel(p, c), Pixel(Point::new(w - 1 - p.x, p.y), c)];

        if self.is_fading {
            let start_time = self.fade_start_time.get_or_insert(current_time);
            let elapsed = current_time.saturating_duration_since(*start_time);
            self.fade_progress = Frac::from_ratio(elapsed as u32, self.fade_duration_ms as u32);

            let mut fading_target = FadingDrawTarget {
                target,
                fade_progress: self.fade_progress,
                target_color: self.background_color,
            };

            let proto = make_iter(self.border_color);
            let top = proto.top_center();
            let bottom = proto.bottom_center();
            fading_target.draw_iter(proto.with_frac_range(top, bottom).flat_map(mirror))
        } else {
            let proto = make_iter(self.border_color);
            let top = proto.top_center();
            let bottom = proto.bottom_center();

            let mut new_progress = self.progress;
            let mut old_progress = self.last_drawn_progress;

            let color = if new_progress > old_progress {
                self.border_color
            } else {
                core::mem::swap(&mut new_progress, &mut old_progress);
                self.background_color
            };

            let old_end = lerp_frac(top, bottom, old_progress);
            let new_end = lerp_frac(top, bottom, new_progress);

            let iter = make_iter(color)
                .with_frac_range(old_end, new_end)
                .flat_map(mirror);
            target.draw_iter(iter)?;

            self.last_drawn_progress = self.progress;
            Ok(())
        }
    }
}
