use super::{Widget, Text, Column};
use crate::{bitmap::{EncodedImage, BitmapWidget}, color_map::ColorMap, palette::PALETTE, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, Rgb565}, text::Alignment,
};
use u8g2_fonts::U8g2TextStyle;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A welcome screen widget showing the Frostsnap logo and getting started text
pub struct Welcome {
    column: Column<(
        ColorMap<BitmapWidget, Rgb565>,
        Text<U8g2TextStyle<Rgb565>>,
        Text<U8g2TextStyle<Rgb565>>,
    )>,
}

impl Welcome {
    pub fn new() -> Self {
        // Create text styles with colors directly
        let text_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_background);
        let url_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.primary);
        
        // Create text widgets with colored styles
        let text1 = Text::new("Get started with\nFrostsnap at", text_style.clone()).with_alignment(Alignment::Center);
        let url_text = Text::new("frostsnap.com/start", url_style).with_underline(PALETTE.primary);
        
        // Load logo
        let image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let bitmap_widget = BitmapWidget::new(image.into());
        let logo = bitmap_widget.color_map(|color| match color {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });
        
        // Create column with spacing
        let column = Column::new((
            logo,
            text1,
            url_text,
        )).with_main_axis_alignment(crate::MainAxisAlignment::SpaceEvenly);
        
        Self { column }
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::DynWidget for Welcome {
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.column.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.column.handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn size_hint(&self) -> Option<Size> {
        self.column.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.column.force_full_redraw();
    }
}

impl Widget for Welcome {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }
}
