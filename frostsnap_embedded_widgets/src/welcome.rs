use super::{Widget, Text, Column, SizedBox};
use crate::{bitmap::{EncodedImage, BitmapWidget}, color_map::ColorMap, palette::PALETTE, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, Rgb565},
};
use u8g2_fonts::U8g2TextStyle;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A welcome screen widget showing the Frostsnap logo and getting started text
pub struct Welcome {
    column: Column<(
        SizedBox<Rgb565>,
        ColorMap<BitmapWidget, Rgb565>,
        SizedBox<Rgb565>,
        ColorMap<Text<U8g2TextStyle<BinaryColor>>, Rgb565>,
        ColorMap<Text<U8g2TextStyle<BinaryColor>>, Rgb565>,
        SizedBox<Rgb565>,
        ColorMap<Text<U8g2TextStyle<BinaryColor>>, Rgb565>,
    ), Rgb565>,
}

impl Welcome {
    pub fn new() -> Self {
        let text_style = U8g2TextStyle::new(crate::FONT_MED, BinaryColor::On);
        
        // Create text widgets
        let text1 = Text::new("Get started with", text_style.clone());
        let text1_colored = text1.color_map(|c| match c {
            BinaryColor::On => PALETTE.on_background,
            BinaryColor::Off => PALETTE.background,
        });
        
        let text2 = Text::new("your Frostsnap at", text_style.clone());
        let text2_colored = text2.color_map(|c| match c {
            BinaryColor::On => PALETTE.on_background,
            BinaryColor::Off => PALETTE.background,
        });
        
        let url_text = Text::new("frostsnap.com/start", text_style);
        let url_colored = url_text.color_map(|c| match c {
            BinaryColor::On => PALETTE.primary_container,
            BinaryColor::Off => PALETTE.background,
        });
        
        // Load logo
        let image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let bitmap_widget = BitmapWidget::new(image.into());
        let logo = bitmap_widget.color_map(|color| match color {
            BinaryColor::On => PALETTE.primary,
            BinaryColor::Off => PALETTE.background,
        });
        
        // Create column with spacing
        let column = Column::new((
            SizedBox::new(Size::new(0, 40)),
            logo,
            SizedBox::new(Size::new(0, 20)),
            text1_colored,
            text2_colored,
            SizedBox::new(Size::new(0, 10)),
            url_colored,
        ));
        
        Self { column }
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
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
