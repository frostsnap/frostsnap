use crate::super_draw_target::SuperDrawTarget;
use core::marker::PhantomData;

use embedded_graphics::{geometry::Size, image::Image, pixelcolor::Rgb565, prelude::*};
use embedded_iconoir::{
    icons::size24px::actions::Check, prelude::IconoirNewIcon, size32px::navigation::NavArrowLeft,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Icon<I> {
    icon: PhantomData<I>,
    color: Rgb565,
    center: Option<Point>,
}

impl<I> Default for Icon<I> {
    fn default() -> Self {
        Self {
            icon: Default::default(),
            color: Default::default(),
            center: Default::default(),
        }
    }
}

impl<I: IconoirNewIcon<Rgb565>> Icon<I> {
    pub fn with_color(mut self, color: Rgb565) -> Self {
        self.color = color;
        self
    }

    pub fn with_center(mut self, center: Point) -> Self {
        self.center = Some(center);
        self
    }

    pub fn draw(&self, target: &mut impl DrawTarget<Color = Rgb565>) {
        let center = self.center.unwrap_or_else(|| {
            let size = target.bounding_box().size;
            Point::new(size.width as i32 / 2, size.height as i32 / 2)
        });
        let icon = I::new(self.color);
        let _ = Image::with_center(&icon, center).draw(target);
    }
}

pub fn backspace() -> Icon<NavArrowLeft> {
    Icon::default()
}

pub fn confirm() -> Icon<Check> {
    Icon::default()
}

/// Wrapper to make an icon into a widget
pub struct IconWidget<I> {
    icon: I,
    constraints: Option<Size>,
    needs_redraw: bool,
}

impl<I: embedded_graphics::image::ImageDrawable<Color = Rgb565>> IconWidget<I> {
    pub fn new(icon: I) -> Self {
        Self {
            icon,
            constraints: None,
            needs_redraw: true,
        }
    }
}

impl<I: embedded_graphics::image::ImageDrawable<Color = Rgb565>> crate::DynWidget
    for IconWidget<I>
{
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        let size = self.icon.bounding_box().size;
        crate::Sizing {
            width: size.width,
            height: size.height,
        }
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl<I: embedded_graphics::image::ImageDrawable<Color = Rgb565>> crate::Widget for IconWidget<I> {
    type Color = Rgb565;

    fn draw<D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.needs_redraw {
            Image::new(&self.icon, Point::zero()).draw(target)?;
            self.needs_redraw = false;
        }
        Ok(())
    }
}
