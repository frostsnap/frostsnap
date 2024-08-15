// 1.69 inch 240x280 ST7789+CST816S

use crate::alloc::string::ToString;
use alloc::string::String;
use embedded_graphics::{
    draw_target::{Cropped, DrawTarget},
    geometry::{AnchorX, AnchorY},
    image::Image,
    mono_font::{ascii::FONT_7X14, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
    text::{Alignment, Text},
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

const HEADER_COLOR: Rgb565 = Rgb565::new(4, 8, 17);
const PADDING_TOP: u32 = 40;
const PADDING_LEFT: u32 = 10;
const FONT_LARGE: fonts::u8g2_font_profont29_mf = fonts::u8g2_font_profont29_mf;
const FONT_MED: fonts::u8g2_font_profont22_mf = fonts::u8g2_font_profont22_mf;
const FONT_SMALL: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;
const TEXTBOX_STYLE: TextBoxStyle = TextBoxStyleBuilder::new().build();
const BODY_RECT: Rectangle = Rectangle::new(
    Point::new(PADDING_LEFT as i32, PADDING_TOP as i32),
    Size::new(240 - PADDING_LEFT * 2, 280 - PADDING_TOP),
);
/// for when you want to write outside the margins
const BODY_RECT_NO_HORIZONTAL_PADDING: Rectangle = Rectangle::new(
    Point::new(0, PADDING_TOP as i32),
    Size::new(240, 280 - PADDING_TOP),
);

type FrameBuffer<'d> = FrameBuf<Rgb565, &'d mut [Rgb565; 67200]>;

pub struct Graphics<'d, DT> {
    display: DT,
    framebuf: FrameBuffer<'d>,
}

impl<'d, DT> Graphics<'d, DT>
where
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    pub fn new(display: DT, framebuf: FrameBuffer<'d>) -> Result<Self, Error> {
        let mut _self = Self { framebuf, display };

        Ok(_self)
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.display.draw_iter(&self.framebuf)
    }

    pub fn clear(&mut self, c: Rgb565) {
        Rectangle::new(Point::new(0, 0), self.display.size())
            .into_styled(PrimitiveStyleBuilder::new().fill_color(c).build())
            .draw(&mut self.framebuf)
            .unwrap();
    }

    fn body_no_horizontal_padding(&mut self) -> Cropped<'_, FrameBuffer<'d>> {
        self.framebuf.cropped(&BODY_RECT_NO_HORIZONTAL_PADDING)
    }
    fn body(&mut self) -> Cropped<'_, FrameBuffer<'d>> {
        self.framebuf.cropped(&BODY_RECT)
    }

    fn raw_body(&mut self) -> Cropped<'_, DT> {
        self.display.cropped(&BODY_RECT)
    }

    pub fn print(&mut self, str: impl AsRef<str>) {
        let mut body = self.body();
        let _overflow = TextBox::with_textbox_style(
            str.as_ref(),
            body.bounding_box(),
            U8g2TextStyle::new(FONT_MED, Rgb565::WHITE),
            TEXTBOX_STYLE,
        )
        .draw(&mut body)
        .unwrap();
    }

    pub fn header(&mut self, device_label: impl AsRef<str>) {
        let header_height = 25;
        Rectangle::new(
            Point::zero(),
            Size::new(self.display.size().width, header_height),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(HEADER_COLOR)
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
                Size::new(self.display.size().width - 20, header_height),
            ),
            U8g2TextStyle::new(FONT_SMALL, Rgb565::WHITE),
            textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();

        Line::new(
            Point::new(0, (header_height - 1) as i32),
            Point::new(self.display.size().width as i32, (header_height - 1) as i32),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_GRAY, 1))
        .draw(&mut self.framebuf)
        .unwrap();
    }

    pub fn confirm_bar(&mut self, percent: f32) {
        let stroke = 3;
        let mut body = self.raw_body();
        let y = body.size().height - stroke - 32;

        Line::new(
            Point::new(71, y as i32),
            Point::new((100_f32 * percent) as i32 + 70, y as i32),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, stroke))
        .draw(&mut body)
        .unwrap();
    }

    pub fn progress_bar(&mut self, percent: f32) {
        let mut body = self.body();
        let bar_y = body.size().height as f32 * 0.8;
        let bar_x = body.size().width as f32 * 0.5;
        let bar_width = body.size().width as f32 * 0.8;
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
            .draw(&mut body)
            .unwrap();

        let progress = border.resized_width((bar_width * percent) as u32, AnchorX::Left);

        progress
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::CSS_REBECCAPURPLE)
                    .build(),
            )
            .draw(&mut body)
            .unwrap();

        embedded_graphics::text::Text::with_alignment(
            &format!("{}%", (percent * 100.0) as u32),
            Point::new(
                (body.size().width / 2) as i32,
                (bar_y as u32 + bar_height + 10) as i32,
            ),
            U8g2TextStyle::new(FONT_MED, Rgb565::WHITE),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();
    }

    pub fn button(&mut self) {
        let mut body = self.body();
        Rectangle::new(
            Point::new(70, body.size().height as i32 - 37),
            Size::new(100, 4),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_width(1)
                .stroke_color(Rgb565::CSS_DARK_GRAY)
                .fill_color(Rgb565::new(7, 14, 7))
                .build(),
        )
        .draw(&mut body)
        .unwrap();

        Rectangle::new(
            Point::new(70, body.size().height as i32 - 34),
            Size::new(100, 34),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_width(1)
                .stroke_color(Rgb565::CSS_DARK_GRAY)
                .build(),
        )
        .draw(&mut body)
        .unwrap();

        let icon = OpenSelectHandGesture::new(Rgb565::GREEN);
        Image::new(&icon, Point::new(108, body.size().height as i32 - 29))
            .draw(&mut body)
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
        let mut body = self.body_no_horizontal_padding();
        let mut y_offset = 0;
        let vertical_spacing = 35;
        let horizontal_spacing = 80; // Separate variable for horizontal spacing
        let (hrp, backup_chars) = str.split_at(str.find(']').expect("backup has a hrp") + 1);
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

        Text::with_alignment(
            "Share backup:",
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_MED, Rgb565::CYAN),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        y_offset += vertical_spacing;

        Text::with_alignment(
            hrp,
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_LARGE, Rgb565::WHITE),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        y_offset += vertical_spacing;

        for row_chunks in chunked_backup.chunks(3) {
            let mut x_offset = PADDING_LEFT as i32;

            for chunk in row_chunks {
                Text::new(
                    chunk,
                    Point::new(x_offset, y_offset),
                    U8g2TextStyle::new(FONT_LARGE, Rgb565::WHITE),
                )
                .draw(&mut body)
                .unwrap();
                x_offset += horizontal_spacing; // Use horizontal spacing variable
            }

            y_offset += vertical_spacing;
        }
    }

    pub fn show_keygen_check(&mut self, check: &str) {
        let mut body = self.body();
        let mut y_offset = 10;
        Text::with_alignment(
            "Keygen check",
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_MED, Rgb565::CYAN),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        y_offset += 35;

        TextBox::with_textbox_style(
            "This must show on all other devices:",
            body.bounding_box()
                .resized_height(body.size().height - y_offset as u32, AnchorY::Bottom),
            U8g2TextStyle::new(FONT_MED, Rgb565::WHITE),
            TEXTBOX_STYLE,
        )
        .draw(&mut body)
        .unwrap();

        y_offset += 85;

        Text::with_alignment(
            check,
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_LARGE, Rgb565::WHITE),
            Alignment::Center,
        )
        .draw(&mut body)
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

    let _ = TextBox::with_textbox_style(
        error.as_ref(),
        Rectangle::new(Point::new(1, (y + 1) as i32), display.size()),
        character_style,
        TEXTBOX_STYLE,
    )
    .draw(display);
}
