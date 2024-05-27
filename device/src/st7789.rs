// 1.69 inch 240x280 ST7789+CST816S

use embedded_graphics::{
    draw_target::DrawTarget,
    image::Image,
    mono_font::{ascii::FONT_7X14, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
};
use embedded_graphics_framebuf::FrameBuf;
use embedded_iconoir::{icons::size24px::gestures::OpenSelectHandGesture, prelude::IconoirNewIcon};
use embedded_text::{
    alignment::HorizontalAlignment,
    style::{TextBoxStyle, TextBoxStyleBuilder},
    TextBox,
};
use mipidsi::error::Error;
use u8g2_fonts::{fonts, U8g2TextStyle};

pub struct Graphics<'d, DT> {
    display: DT,
    textbox_style: TextBoxStyle,
    framebuf: FrameBuf<Rgb565, &'d mut [Rgb565; 67200]>,
}

impl<'d, DT> Graphics<'d, DT>
where
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    pub fn new(
        display: DT,
        framebuf: FrameBuf<Rgb565, &'d mut [Rgb565; 67200]>,
    ) -> Result<Self, Error> {
        // println!("graphics init");

        let textbox_style = TextBoxStyleBuilder::new()
            // .alignment(HorizontalAlignment::Center)
            // .vertical_alignment(VerticalAlignment::Middle)
            // .line_height(LineHeight::Pixels(16))
            .build();

        let mut _self = Self {
            display,
            textbox_style,
            framebuf,
        };

        Ok(_self)
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.display
            // .fill_contiguous(&area, self.framebuf.data)
            .draw_iter(&self.framebuf)
    }

    pub fn clear(&mut self, c: Rgb565) {
        Rectangle::new(Point::new(0, 0), self.display.size())
            .into_styled(PrimitiveStyleBuilder::new().fill_color(c).build())
            .draw(&mut self.display)
            .unwrap();
    }

    pub fn print(&mut self, str: impl AsRef<str>) {
        let y = 40;
        let x_offset = 0;
        let body_area = Size::new(self.display.size().width, self.display.size().height - y);
        Rectangle::new(Point::new(x_offset, y as i32), body_area)
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        let _overflow = TextBox::with_textbox_style(
            str.as_ref(),
            Rectangle::new(Point::new(x_offset, y as i32), body_area),
            U8g2TextStyle::new(fonts::u8g2_font_profont29_mf, Rgb565::WHITE),
            // U8g2TextStyle::new(fonts::u8g2_font_helvR14_tf, Rgb565::WHITE),
            // U8g2TextStyle::new(fonts::u8g2_font_spleen12x24_mf, Rgb565::WHITE),
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();

        // self.flush().unwrap();
    }

    pub fn header(&mut self, device_label: impl AsRef<str>) {
        let y = 25;
        Rectangle::new(Point::zero(), Size::new(self.display.size().width, y))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::new(4, 8, 17))
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        let textbox_style = TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .build();
        TextBox::with_textbox_style(
            device_label.as_ref(),
            Rectangle::new(
                Point::new(10, 7),
                Size::new(self.display.size().width - 20, y),
            ),
            U8g2TextStyle::new(fonts::u8g2_font_profont17_mf, Rgb565::WHITE),
            // U8g2TextStyle::new(fonts::u8g2_font_helvR08_tf, Rgb565::WHITE),
            // U8g2TextStyle::new(fonts::u8g2_font_spleen5x8_mf, Rgb565::WHITE),
            textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();

        Line::new(
            Point::new(0, (y - 1) as i32),
            Point::new(self.display.size().width as i32, (y - 1) as i32),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_GRAY, 1))
        .draw(&mut self.framebuf)
        .unwrap();
    }

    pub fn confirm_bar(&mut self, percent: f32) {
        let stroke = 3;
        let y = self.display.size().height - stroke - 32;

        Line::new(
            Point::new(71, y as i32),
            Point::new((100_f32 * percent) as i32 + 70, y as i32),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, stroke))
        .draw(&mut self.display)
        .unwrap();
    }

    pub fn button(&mut self) {
        Rectangle::new(
            Point::new(70, self.display.size().height as i32 - 37),
            Size::new(100, 4),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_width(1)
                .stroke_color(Rgb565::CSS_DARK_GRAY)
                .fill_color(Rgb565::new(7, 14, 7))
                .build(),
        )
        .draw(&mut self.framebuf)
        .unwrap();

        Rectangle::new(
            Point::new(70, self.display.size().height as i32 - 34),
            Size::new(100, 34),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_width(1)
                .stroke_color(Rgb565::CSS_DARK_GRAY)
                .build(),
        )
        .draw(&mut self.framebuf)
        .unwrap();

        let icon = OpenSelectHandGesture::new(Rgb565::GREEN);
        Image::new(
            &icon,
            Point::new(108, self.display.size().height as i32 - 29),
        )
        .draw(&mut self.framebuf)
        .unwrap();
    }

    pub fn upstream_state(&mut self, color: Rgb565, is_device: bool) {
        let arrow = Triangle::new(Point::new(20, 20), Point::new(30, 20), Point::new(25, 7));

        if is_device {
            arrow.into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(color)
                    .stroke_width(1)
                    .build(),
            )
        } else {
            arrow.into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
        }
        .draw(&mut self.framebuf)
        .unwrap();
    }

    pub fn downstream_state(&mut self, color: Option<Rgb565>) {
        Triangle::new(Point::new(32, 7), Point::new(42, 7), Point::new(37, 20))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(color.unwrap_or(Rgb565::new(4, 8, 17) /* background color */))
                    .stroke_width(1)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();
    }

    pub fn set_mem_debug(&mut self, used: usize, free: usize) {
        let display = &self.display;
        let point = Point::new(4, 26);
        let size = Size::new(display.size().width, 20);

        Rectangle::new(point, size)
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        TextBox::with_textbox_style(
            &format!("{}/{}", used, free),
            Rectangle::new(point, size),
            MonoTextStyle::new(&FONT_7X14, Rgb565::GREEN),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Left)
                .build(),
        )
        .draw(&mut self.framebuf)
        .unwrap();
    }
}

pub fn error_print<DT>(display: &mut DT, error: impl AsRef<str>)
where
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    let y = 25;
    let header_area = Rectangle::new(Point::zero(), Size::new(display.size().width, y));
    let _ = header_area
        .into_styled(PrimitiveStyleBuilder::new().fill_color(Rgb565::RED).build())
        .draw(display);

    let header_charstyle = MonoTextStyle::new(&FONT_7X14, Rgb565::WHITE);
    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .build();
    let _ = TextBox::with_textbox_style(
        "ERROR",
        Rectangle::new(Point::new(1, 9), Size::new(display.size().width, y)),
        header_charstyle,
        textbox_style,
    )
    .draw(display);

    Line::new(
        Point::new(0, y as i32),
        Point::new(display.size().width as i32, y as i32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_GRAY, 1))
    .draw(display)
    .unwrap();

    let _ = Rectangle::new(
        Point::new(0, (y + 1) as i32),
        Size::new(display.size().width, display.size().height),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::BLACK)
            .build(),
    )
    .draw(display);

    let character_style = MonoTextStyle::new(&FONT_7X14, Rgb565::WHITE);
    let textbox_style = TextBoxStyleBuilder::new().build();

    let _ = TextBox::with_textbox_style(
        error.as_ref(),
        Rectangle::new(Point::new(1, (y + 1) as i32), display.size()),
        character_style,
        textbox_style,
    )
    .draw(display);
}
