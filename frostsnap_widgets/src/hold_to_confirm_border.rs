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

/// A border that fills progressively around a rounded rectangle as the user
/// holds down on the button. Draws only the left half of the perimeter and
/// mirrors horizontally to produce a symmetric animation.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    pub child: W,
    /// The current progress as a fraction (0→1), set by the caller.
    progress: Frac,
    /// The pixel index that `progress` maps to in the half-perimeter iterator.
    target_pixel: u32,
    /// The pixel index up to which the border has been drawn on screen.
    last_drawn_pixel: u32,
    /// Total visible pixels in the half-perimeter (top_center → bottom_center).
    /// Computed lazily on first draw.
    half_pixel_count: u32,
    screen_size: Option<Size>,
    sizing: crate::Sizing,
    border_width: u32,
    border_color: C,
    needs_full_redraw: bool,
    /// How far through the fade-out animation (0→1).
    fade_progress: Frac,
    fade_start_time: Option<crate::Instant>,
    fade_duration_ms: u64,
    is_fading: bool,
}

impl<W, C> HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor + crate::widget_color::ColorInterpolate,
{
    pub fn new(child: W, border_width: u32, border_color: C) -> Self {
        Self {
            child,
            progress: Frac::ZERO,
            target_pixel: 0,
            last_drawn_pixel: 0,
            half_pixel_count: 0,
            screen_size: None,
            sizing: crate::Sizing {
                width: 0,
                height: 0,
                ..Default::default()
            },
            border_width,
            border_color,
            needs_full_redraw: false,
            fade_progress: Frac::ZERO,
            fade_start_time: None,
            fade_duration_ms: 0,
            is_fading: false,
        }
    }

    pub fn set_progress(&mut self, progress: Frac) {
        self.progress = progress;
        self.target_pixel = (progress * self.half_pixel_count).round();
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
}

impl<W, C> crate::DynWidget for HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor + crate::widget_color::ColorInterpolate,
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

        let dummy = self.border_color;
        let proto = AARoundedRectIter::new(
            max_size.width,
            max_size.height,
            CORNER_RADIUS,
            self.border_width,
            dummy,
            dummy,
            dummy,
        );
        let top = proto.top_center();
        let bottom = proto.bottom_center();
        self.half_pixel_count = proto.with_frac_range(top, bottom).count() as u32;
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
        self.needs_full_redraw = true;
        self.child.force_full_redraw();
    }
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
        let child_area = Rectangle::new(offset, child_size.into());
        let mut cropped = target.clone().crop(child_area);
        self.child.draw(&mut cropped, current_time)?;

        if self.is_faded_out() {
            return Ok(());
        }

        let size = self.screen_size.unwrap();
        let bg = target.background_color();
        let make_half_iter = |color: Rgb565| -> AARoundedRectIter<Rgb565> {
            let proto = AARoundedRectIter::new(
                size.width,
                size.height,
                CORNER_RADIUS,
                self.border_width,
                color,
                bg,
                bg,
            );
            let top = proto.top_center();
            let bottom = proto.bottom_center();
            proto.with_frac_range(top, bottom)
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
                target_color: bg,
            };

            fading_target.draw_iter(make_half_iter(self.border_color).flat_map(mirror))
        } else {
            let draw_pixels =
                |target: &mut SuperDrawTarget<D, Rgb565>, from: u32, to: u32, color: Rgb565| {
                    let iter = make_half_iter(color)
                        .skip(from as usize)
                        .take((to - from) as usize)
                        .flat_map(&mirror);
                    target.draw_iter(iter)
                };

            if self.needs_full_redraw {
                self.needs_full_redraw = false;
                draw_pixels(target, 0, self.target_pixel, self.border_color)?;
            } else if self.target_pixel > self.last_drawn_pixel {
                draw_pixels(
                    target,
                    self.last_drawn_pixel,
                    self.target_pixel,
                    self.border_color,
                )?;
            }

            if self.target_pixel < self.last_drawn_pixel {
                draw_pixels(target, self.target_pixel, self.last_drawn_pixel, bg)?;
            }

            self.last_drawn_pixel = self.target_pixel;
            Ok(())
        }
    }
}
