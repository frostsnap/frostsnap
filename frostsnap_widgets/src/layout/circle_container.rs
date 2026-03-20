use crate::aa::circle::AACircle;
use crate::super_draw_target::SuperDrawTarget;
use crate::widget_color::ColorInterpolate;
use crate::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    prelude::*,
    primitives::Rectangle,
    Pixel,
};

#[derive(Clone)]
pub struct CircleContainer<W: Widget>
where
    W::Color: ColorInterpolate,
{
    pub child: W,
    radius: u32,
    stroke_width: u32,
    fill_color: W::Color,
    stroke_color: W::Color,
    needs_redraw: bool,
}

impl<W: Widget> CircleContainer<W>
where
    W::Color: ColorInterpolate,
{
    pub fn new(child: W, radius: u32, fill_color: W::Color, stroke_color: W::Color) -> Self {
        Self {
            child,
            radius,
            stroke_width: 2,
            fill_color,
            stroke_color,
            needs_redraw: true,
        }
    }

    pub fn with_stroke_width(mut self, stroke_width: u32) -> Self {
        self.stroke_width = stroke_width;
        self
    }

    pub fn set_fill(&mut self, color: W::Color) {
        self.fill_color = color;
        self.needs_redraw = true;
        self.child.force_full_redraw();
    }

    pub fn set_stroke(&mut self, color: W::Color) {
        self.stroke_color = color;
        self.needs_redraw = true;
    }

    pub fn set_colors(&mut self, fill: W::Color, stroke: W::Color) {
        self.fill_color = fill;
        self.stroke_color = stroke;
        self.needs_redraw = true;
        self.child.force_full_redraw();
    }

    fn diameter(&self) -> u32 {
        self.radius * 2
    }

    fn clip_radius(&self) -> i32 {
        // 🎯 clip inside the stroke so child doesn't overwrite AA border
        (self.radius.saturating_sub(self.stroke_width + 2)) as i32
    }
}

impl<W: Widget> crate::DynWidget for CircleContainer<W>
where
    W::Color: ColorInterpolate,
{
    fn set_constraints(&mut self, _max_size: Size) {
        let d = self.diameter();
        self.child.set_constraints(Size::new(d, d));
    }

    fn sizing(&self) -> crate::Sizing {
        let d = self.diameter();
        Size::new(d, d).into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, is_release)
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
        self.child.force_full_redraw();
    }
}

impl<W: Widget> Widget for CircleContainer<W>
where
    W::Color: ColorInterpolate,
{
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if self.needs_redraw {
            let center = Point::new(self.radius as i32, self.radius as i32);
            let draw_radius = self.radius.saturating_sub(self.stroke_width);

            AACircle::new(
                center,
                draw_radius,
                self.stroke_width,
                self.fill_color,
                self.stroke_color,
                target.background_color(),
            )
            .draw(target)?;

            self.needs_redraw = false;
        }

        let d = self.diameter();
        let child_sizing = self.child.sizing();
        let child_size: Size = child_sizing.into();
        let child_x = ((d as i32 - child_size.width as i32) / 2).max(0);
        let child_y = ((d as i32 - child_size.height as i32) / 2).max(0);

        let child_rect = Rectangle::new(Point::new(child_x, child_y), child_size);
        let cropped = target
            .clone()
            .crop(child_rect)
            .with_background_color(self.fill_color);

        // 🎯 adjust center to child's coordinate space (crop shifts the origin)
        let center = Point::new(self.radius as i32 - child_x, self.radius as i32 - child_y);
        let clip_r = self.clip_radius();
        let clipped = CircleClipTarget {
            inner: cropped,
            center,
            clip_radius_sq: (clip_r as i64) * (clip_r as i64),
        };

        let mut child_target = SuperDrawTarget::new(clipped, self.fill_color);
        self.child.draw(&mut child_target, current_time)?;

        Ok(())
    }
}

struct CircleClipTarget<D> {
    inner: D,
    center: Point,
    clip_radius_sq: i64,
}

impl<D: DrawTarget> DrawTarget for CircleClipTarget<D> {
    type Color = D::Color;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let center = self.center;
        let clip_radius_sq = self.clip_radius_sq;
        self.inner
            .draw_iter(pixels.into_iter().filter(move |Pixel(point, _)| {
                let dx = (point.x - center.x) as i64;
                let dy = (point.y - center.y) as i64;
                dx * dx + dy * dy <= clip_radius_sq
            }))
    }
}

impl<D: DrawTarget> Dimensions for CircleClipTarget<D> {
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}
