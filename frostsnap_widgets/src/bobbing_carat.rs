use crate::{image::Image, translate::Translate};
use embedded_graphics::{geometry::Point, pixelcolor::Rgb565};
use embedded_iconoir::prelude::IconoirNewIcon;
use frostsnap_macros::Widget;

/// A 16px bobbing carat icon that bobs up and down
#[derive(Widget)]
pub struct BobbingCarat {
    #[widget_delegate]
    translate: Translate<
        Image<
            embedded_iconoir::Icon<
                Rgb565,
                embedded_iconoir::icons::size16px::navigation::NavArrowUp,
            >,
        >,
    >,
}

impl BobbingCarat {
    pub fn new(color: Rgb565, background_color: Rgb565) -> Self {
        let icon = embedded_iconoir::icons::size16px::navigation::NavArrowUp::new(color);
        let image_widget = Image::new(icon);
        let mut translate = Translate::new(image_widget, background_color);

        // Set up bobbing animation - move up and down by 5 pixels over 1 second
        translate.set_repeat(true);
        translate.animate_to(Point::new(0, 5), 500); // 500ms down, 500ms back up

        Self { translate }
    }
}
