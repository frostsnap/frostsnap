use crate::super_draw_target::SuperDrawTarget;
use crate::{image::Image, translate::Translate, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
};
use embedded_iconoir::{prelude::IconoirNewIcon, size24px::navigation::NavArrowUp};

/// A carat icon that bobs up and down
pub struct BobbingCarat<C: crate::WidgetColor> {
    translate: Translate<Image<embedded_iconoir::Icon<C, NavArrowUp>>>,
}

impl<C: crate::WidgetColor> BobbingCarat<C>
where
    C: Copy,
{
    pub fn new(color: C, background_color: C) -> Self {
        let icon = NavArrowUp::new(color);
        let image_widget = Image::new(icon);
        let mut translate = Translate::new(image_widget, background_color);

        // Set up bobbing animation - move up and down by 5 pixels over 1 second
        translate.set_repeat(true);
        translate.animate_to(Point::new(0, 5), 500); // 500ms down, 500ms back up

        Self { translate }
    }
}

impl<C: crate::WidgetColor> crate::DynWidget for BobbingCarat<C>
where
    C: Copy,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.translate.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.translate.sizing()
    }

    fn force_full_redraw(&mut self) {
        self.translate.force_full_redraw();
    }
}

impl<C: crate::WidgetColor> Widget for BobbingCarat<C>
where
    C: Copy,
{
    type Color = C;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.translate.draw(target, current_time)
    }
}
