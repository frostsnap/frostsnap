use core::marker::PhantomData;

use embedded_graphics::{image::Image, pixelcolor::Rgb565, prelude::*};
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
