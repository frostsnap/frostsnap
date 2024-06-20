// 1.69 inch 240x280 ST7789+CST816S

use crate::alloc::string::ToString;
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::AnchorX,
    image::Image,
    mono_font::{ascii::FONT_7X14, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
    text::Alignment,
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

use crate::{DownstreamConnectionState, UpstreamConnectionState};

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
            U8g2TextStyle::new(fonts::u8g2_font_profont22_mf, Rgb565::WHITE),
            // U8g2TextStyle::new(fonts::u8g2_font_helvR14_tf, Rgb565::WHITE),
            // U8g2TextStyle::new(fonts::u8g2_font_spleen12x24_mf, Rgb565::WHITE),
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();
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

    pub fn progress_bar(&mut self, percent: f32) {
        let bar_y = self.display.size().height as f32 * 0.8;
        let bar_x = self.display.size().width as f32 * 0.5;
        let bar_width = self.display.size().width as f32 * 0.8;
        let bar_height = 20;

        let border = Rectangle::with_center(
            Point::new(bar_x as i32, bar_y as i32),
            Size::new(bar_width as u32, bar_height),
        );

        border
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::CSS_DARK_GRAY)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        let progress = border.resized_width((bar_width * percent) as u32, AnchorX::Left);

        progress
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::CSS_REBECCAPURPLE)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        embedded_graphics::text::Text::with_alignment(
            &format!("{}%", (percent * 100.0) as u32),
            Point::new(
                (self.display.size().width / 2) as i32,
                (bar_y as u32 + bar_height + 10) as i32,
            ),
            U8g2TextStyle::new(fonts::u8g2_font_profont22_mf, Rgb565::WHITE),
            Alignment::Center,
        )
        .draw(&mut self.framebuf)
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

    pub fn upstream_state(&mut self, connection_state: UpstreamConnectionState) {
        let color = match connection_state {
            UpstreamConnectionState::Connected => Rgb565::CSS_DIM_GRAY,
            UpstreamConnectionState::Established
            | UpstreamConnectionState::EstablishedAndCoordAck => Rgb565::GREEN,
        };
        let arrow = Triangle::new(Point::new(20, 20), Point::new(30, 20), Point::new(25, 7));
        arrow
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(&mut self.framebuf)
            .unwrap();

        let circle = Circle::with_center(Point::new(50, 13), 10);
        let color = match connection_state {
            UpstreamConnectionState::Connected => Rgb565::CSS_DIM_GRAY,
            UpstreamConnectionState::Established => Rgb565::CSS_ORANGE,
            UpstreamConnectionState::EstablishedAndCoordAck => Rgb565::GREEN,
        };
        circle
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(&mut self.framebuf)
            .unwrap();
    }

    pub fn downstream_state(&mut self, connection_state: DownstreamConnectionState) {
        let color = match connection_state {
            DownstreamConnectionState::Disconnected => Rgb565::CSS_DIM_GRAY,
            DownstreamConnectionState::Connected => Rgb565::CSS_ORANGE,
            DownstreamConnectionState::Established => Rgb565::GREEN,
        };
        Triangle::new(Point::new(32, 7), Point::new(42, 7), Point::new(37, 20))
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
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

    pub fn show_backup(&mut self, str: alloc::string::String) {
        let y = 40;
        let mut x_offset = 0;
        let mut y_offset = 0;
        let spacing_size = 20;
        let body_area = Size::new(self.display.size().width, self.display.size().height - y);
        Rectangle::new(Point::new(x_offset, y as i32), body_area)
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        let (hrp, backup_chars) = str.split_at(str.find('1').expect("backup has a hrp") + 1);
        let chunked_backup =
            backup_chars
                .chars()
                .fold(vec![String::new()], |mut chunk_vec, char| {
                    if chunk_vec.last().unwrap().len() < 4 {
                        let last = chunk_vec.last_mut().unwrap();
                        last.push(char);
                    } else {
                        chunk_vec.push(char.to_string());
                    }
                    chunk_vec
                });

        let _overflow = TextBox::with_textbox_style(
            "Share backup:",
            Rectangle::new(Point::new(x_offset, (y as i32) + y_offset), body_area),
            U8g2TextStyle::new(fonts::u8g2_font_profont29_mf, Rgb565::WHITE),
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();
        y_offset += spacing_size * 2;

        let _overflow = TextBox::with_textbox_style(
            hrp,
            Rectangle::new(Point::new(x_offset, (y as i32) + y_offset), body_area),
            U8g2TextStyle::new(fonts::u8g2_font_profont29_mf, Rgb565::CYAN),
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();
        y_offset += spacing_size * 3 / 2;

        for (i, chunk) in chunked_backup.into_iter().enumerate() {
            let _overflow = TextBox::with_textbox_style(
                chunk.as_ref(),
                Rectangle::new(Point::new(x_offset, (y as i32) + y_offset), body_area),
                U8g2TextStyle::new(fonts::u8g2_font_profont29_mf, Rgb565::WHITE),
                self.textbox_style,
            )
            .draw(&mut self.framebuf)
            .unwrap();
            x_offset += spacing_size * 4;
            // For rows of 3, we want a new line for the 4th, 7th, ... chunk
            if (i + 1) % 3 == 0 {
                y_offset += spacing_size * 3 / 2;
                x_offset = 0;
            }
        }
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
