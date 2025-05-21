// For use with 1.69 inch 240x280 ST7789+CST816S

pub mod palette;
use crate::alloc::string::ToString;
use crate::{DownstreamConnectionState, UpstreamConnectionState};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use embedded_graphics::framebuffer::{buffer_size, Framebuffer};
use embedded_graphics::pixelcolor::raw::{LittleEndian, RawU16};
use embedded_graphics::{
    draw_target::{Cropped, DrawTarget},
    geometry::{AnchorX, AnchorY},
    image::Image,
    image::ImageDrawable,
    mono_font::{ascii, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
    text::{Alignment, Text},
};
use embedded_iconoir::{icons::size24px::gestures::OpenSelectHandGesture, prelude::IconoirNewIcon};
use embedded_text::{
    alignment::HorizontalAlignment,
    style::{TextBoxStyle, TextBoxStyleBuilder},
    TextBox,
};
use mipidsi::error::Error;
use palette::COLORS;
use u8g2_fonts::{fonts, U8g2TextStyle};
pub mod animation;
pub mod widgets;

pub const PADDING_TOP: u32 = 40;
pub const PADDING_LEFT: u32 = 10;
pub const FONT_LARGE: fonts::u8g2_font_profont29_mf = fonts::u8g2_font_profont29_mf;
pub const FONT_MED: fonts::u8g2_font_profont22_mf = fonts::u8g2_font_profont22_mf;
pub const FONT_SMALL: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;
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

pub type Fb =
    Framebuffer<Rgb565, RawU16, LittleEndian, 240, 280, { buffer_size::<Rgb565>(240, 280) }>;

pub struct Graphics<DT> {
    pub display: DT,
    pub framebuf: Box<Fb>,
}

impl<DT> Graphics<DT>
where
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    pub fn new(display: DT) -> Self {
        Self {
            framebuf: Box::new(Fb::new()),
            display,
        }
    }

    pub fn flush(&mut self) {
        self.framebuf.as_image().draw(&mut self.display).unwrap()
    }

    pub fn clear(&mut self) {
        Rectangle::new(Point::new(0, 0), self.display.size())
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLORS.background)
                    .build(),
            )
            .draw(&mut *self.framebuf)
            .unwrap();
    }

    fn body_no_horizontal_padding(&mut self) -> Cropped<'_, Fb> {
        self.framebuf
            .as_mut()
            .cropped(&BODY_RECT_NO_HORIZONTAL_PADDING)
    }
    fn body(&mut self) -> Cropped<'_, Fb> {
        self.framebuf.as_mut().cropped(&BODY_RECT)
    }

    pub fn print(&mut self, str: impl AsRef<str>) {
        let mut body = self.body();
        let _overflow = TextBox::with_textbox_style(
            str.as_ref(),
            body.bounding_box(),
            U8g2TextStyle::new(FONT_MED, COLORS.primary),
            TEXTBOX_STYLE,
        )
        .draw(&mut body)
        .unwrap();
    }

    pub fn header(&mut self, device_label: impl AsRef<str>) {
        let header_height = 25;

        let textbox_style = TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .build();
        TextBox::with_textbox_style(
            device_label.as_ref(),
            Rectangle::new(
                Point::new(10, 7),
                Size::new(self.display.size().width - 20, header_height),
            ),
            U8g2TextStyle::new(FONT_SMALL, COLORS.secondary),
            textbox_style,
        )
        .draw(&mut *self.framebuf)
        .unwrap();
    }

    pub fn confirm_bar(&mut self, percent: f32) {
        let stroke = 3;
        let y = 27;

        Line::new(Point::new(0, y), Point::new((240_f32 * percent) as i32, y))
            .into_styled(PrimitiveStyle::with_stroke(COLORS.success, stroke))
            .draw(&mut self.display)
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
            U8g2TextStyle::new(FONT_MED, COLORS.primary),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();
    }

    pub fn button(&mut self) {
        self.button_with_image(&OpenSelectHandGesture::new(COLORS.primary));
    }

    pub fn button_with_image<IMG: ImageDrawable<Color = Rgb565>>(&mut self, img: &IMG) {
        let mut body = self.body();
        let y = 20;
        let p = Point::new(body.size().width as i32 / 2, body.size().height as i32 - y);
        let rect = Rectangle::with_center(p, Size::new_equal(40));

        Image::with_center(img, rect.center())
            .draw(&mut body)
            .unwrap();
    }

    pub fn upstream_state(&mut self, connection_state: UpstreamConnectionState) {
        let color = match connection_state {
            UpstreamConnectionState::PowerOn => COLORS.secondary,
            UpstreamConnectionState::Established
            | UpstreamConnectionState::EstablishedAndCoordAck => COLORS.success,
        };
        let arrow = Triangle::new(Point::new(20, 20), Point::new(30, 20), Point::new(25, 7));
        arrow
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(&mut *self.framebuf)
            .unwrap();

        let circle = Circle::with_center(Point::new(50, 13), 10);
        let color = match connection_state {
            UpstreamConnectionState::PowerOn => COLORS.secondary,
            UpstreamConnectionState::Established => COLORS.warning,
            UpstreamConnectionState::EstablishedAndCoordAck => COLORS.success,
        };
        circle
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(&mut *self.framebuf)
            .unwrap();
    }

    pub fn downstream_state(&mut self, connection_state: DownstreamConnectionState) {
        let color = match connection_state {
            DownstreamConnectionState::Disconnected => COLORS.secondary,
            DownstreamConnectionState::Connected => COLORS.warning,
            DownstreamConnectionState::Established => COLORS.success,
        };
        Triangle::new(Point::new(32, 7), Point::new(42, 7), Point::new(37, 20))
            .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
            .draw(&mut *self.framebuf)
            .unwrap();
    }

    pub fn set_mem_debug(&mut self, used: usize, free: usize) {
        let display = &self.display;
        let point = Point::new(4, 26);
        let size = Size::new(display.size().width, 20);

        Rectangle::new(point, size)
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLORS.background)
                    .build(),
            )
            .draw(&mut *self.framebuf)
            .unwrap();

        TextBox::with_textbox_style(
            &format!("{}/{}", used, free),
            Rectangle::new(point, size),
            MonoTextStyle::new(&ascii::FONT_7X14, COLORS.success),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Left)
                .build(),
        )
        .draw(&mut *self.framebuf)
        .unwrap();
    }

    pub fn show_backup(&mut self, str: alloc::string::String, show_hint: bool) {
        let str = str.to_uppercase();
        let mut body = self.body_no_horizontal_padding();
        let mut y_offset = 0;
        let vertical_spacing = 35;
        let horizontal_spacing = 80; // Separate variable for horizontal spacing
        let (hrp, backup_chars) = str.split_at(str.find(']').expect("backup has a hrp") + 1);
        let chunked_backup = chunk_string(backup_chars[1..].to_string(), 4); // skip 1

        if show_hint {
            Text::with_alignment(
                "Share backup:",
                Point::new((body.size().width / 2) as i32, y_offset),
                U8g2TextStyle::new(FONT_MED, COLORS.info),
                Alignment::Center,
            )
            .draw(&mut body)
            .unwrap();
            y_offset += vertical_spacing;
        } else {
            y_offset += vertical_spacing / 2;
        }

        Text::with_alignment(
            hrp,
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
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
                    U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
                )
                .draw(&mut body)
                .unwrap();
                x_offset += horizontal_spacing; // Use horizontal spacing variable
            }

            y_offset += vertical_spacing;
        }
    }

    pub fn show_keygen_check(&mut self, name: &str, t_of_n: (u16, u16), check: &str) {
        let mut body = self.body();
        let mut y_offset = 15;
        Text::with_alignment(
            name,
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_MED, COLORS.info),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        y_offset += 35;

        TextBox::with_textbox_style(
            "This must show on all other devices:",
            body.bounding_box()
                .resized_height(body.size().height - y_offset as u32, AnchorY::Bottom),
            U8g2TextStyle::new(FONT_MED, COLORS.primary),
            TEXTBOX_STYLE,
        )
        .draw(&mut body)
        .unwrap();

        y_offset += 85;

        Text::with_alignment(
            check,
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        y_offset += 35;

        Text::with_alignment(
            &format!("{}-of-{}", t_of_n.0, t_of_n.1),
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_MED, COLORS.primary),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();
    }

    pub fn show_keygen_pending_finalize(&mut self, name: &str, t_of_n: (u16, u16), check: &str) {
        self.show_keygen_check(name, t_of_n, check);
        let img = embedded_iconoir::icons::size32px::actions::CheckCircle::new(COLORS.info);
        self.button_with_image(&img);
    }

    pub fn new_device(&mut self) {
        // Define the button's properties
        let button_width = 140;
        let button_height = 50i32;
        let corner_radius = 5;
        let button_color = Rgb565::new(0x04, 0x10, 0x0A);
        let text_color = Rgb565::WHITE;

        // Create a drawing target (like a display buffer)
        let mut body = self.body();

        // Draw the button background as a rounded rectangle
        let button_background = RoundedRectangle::with_equal_corners(
            Rectangle::with_center(
                body.bounding_box().center(),
                Size::new(button_width, button_height as u32),
            ),
            Size::new(corner_radius, corner_radius),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(button_color)
                .build(),
        );

        // Draw the button background on the display
        button_background.draw(&mut body).unwrap();

        // Define the text style
        let text_style = MonoTextStyle::new(&ascii::FONT_10X20, text_color);

        let mut text_pos = body.bounding_box().center();
        text_pos.y += 5;
        // Draw the centered text on the button
        Text::with_alignment(
            "New device",
            text_pos, // Center point for the text
            text_style,
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        // Draw instruction text above the button
        let instruction_text = "Press in the app";
        let instruction_text_style = U8g2TextStyle::new(FONT_MED, COLORS.primary);
        let instruction_position = Point::new(body.bounding_box().center().x, text_pos.y - 90);
        Text::with_alignment(
            instruction_text,
            instruction_position,
            instruction_text_style,
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        // Draw an arrow pointing from the instruction text to the button
        let arrow_start = instruction_position + Point::new(0, 15); // Start below the text
        let arrow_end = body.bounding_box().center() - Point::new(0, button_height / 2 + 10); // End just above the button
        Line::new(arrow_start, arrow_end)
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 4))
            .draw(&mut body)
            .unwrap();

        // Draw the arrowhead pointing down
        let arrow_head_left = arrow_end - Point::new(5, 5);
        let arrow_head_right = arrow_end + Point::new(5, -5);
        Line::new(arrow_end, arrow_head_left)
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 4))
            .draw(&mut body)
            .unwrap();
        Line::new(arrow_end, arrow_head_right)
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 4))
            .draw(&mut body)
            .unwrap();
    }

    pub fn ready_screen(&mut self, name: &str, recovery_mode: bool) {
        let mut body = self.body();

        if recovery_mode {
            Text::with_alignment(
                "Restoration Mode",
                Point::new(
                    (body.size().width / 2) as i32,
                    (body.size().height / 3) as i32,
                ),
                U8g2TextStyle::new(FONT_MED, COLORS.warning),
                Alignment::Center,
            )
            .draw(&mut body)
            .unwrap();
        }

        Text::with_alignment(
            name,
            Point::new(
                (body.size().width / 2) as i32,
                (body.size().height / 2) as i32,
            ),
            U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();
    }

    pub fn show_address(&mut self, address: &str, derivation_path: &str, rand_seed: u32) {
        let mut body = self.body();
        let mut y_offset = 15;
        let mut x_offset = 0;
        let vertical_spacing = 35_i32;

        let chunked_address = chunk_string(address.to_string(), 4);
        let available_chunks = chunked_address.len() - 2; // exclude first and last

        let highlight_index1 = (rand_seed as usize % available_chunks) + 1;
        let highlight_index2 = ((rand_seed.rotate_right(16)) as usize % available_chunks) + 1;

        // if we happened to get the same index, shift the second one
        let highlight_index2 = if highlight_index2 == highlight_index1 {
            (highlight_index2 + 1) % available_chunks + 1
        } else {
            highlight_index2
        };

        let mut i = 0;
        for row_chunks in chunked_address.chunks(3) {
            for item in row_chunks {
                let text_color = if i == highlight_index1 || i == highlight_index2 {
                    COLORS.info
                } else {
                    COLORS.primary
                };

                // centre align last one
                if item == chunked_address.last().unwrap() {
                    Text::with_alignment(
                        item,
                        Point::new((body.size().width / 2) as i32, y_offset),
                        U8g2TextStyle::new(FONT_LARGE, text_color),
                        Alignment::Center,
                    )
                    .draw(&mut body)
                    .unwrap();
                } else {
                    Text::new(
                        item,
                        Point::new(x_offset, y_offset),
                        U8g2TextStyle::new(FONT_LARGE, text_color),
                        // Alignment::Center,
                    )
                    .draw(&mut body)
                    .unwrap();
                }

                x_offset += 80;
                i += 1;
            }
            y_offset += vertical_spacing;
            x_offset = 5;
        }

        y_offset += 5;
        Text::with_alignment(
            derivation_path,
            Point::new((body.size().width / 2) as i32, y_offset),
            U8g2TextStyle::new(FONT_SMALL, COLORS.secondary),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();
    }

    pub fn wipe_data_warning(&mut self) {
        let mut body = self.body();
        Text::with_alignment(
            "WARNING",
            Point::new((body.size().width / 2) as i32, 10),
            U8g2TextStyle::new(FONT_LARGE, COLORS.error),
            Alignment::Center,
        )
        .draw(&mut body)
        .unwrap();

        TextBox::with_textbox_style(
            "Confirming will WIPE all device data.\n\nDELETING ALL SECRETS AND KEY DATA.",
            Rectangle::new(
                Point { x: 0, y: 30 },
                Size::new(body.size().width, body.size().height),
            ),
            U8g2TextStyle::new(FONT_MED, COLORS.primary),
            TEXTBOX_STYLE,
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
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(COLORS.error)
                .build(),
        )
        .draw(display);

    let header_charstyle = MonoTextStyle::new(&ascii::FONT_7X14, COLORS.primary);
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
            .fill_color(COLORS.background)
            .build(),
    )
    .draw(display);

    let character_style = MonoTextStyle::new(&ascii::FONT_7X14, COLORS.primary);

    let _ = TextBox::with_textbox_style(
        error.as_ref(),
        Rectangle::new(Point::new(1, (y + 1) as i32), display.size()),
        character_style,
        TEXTBOX_STYLE,
    )
    .draw(display);
}

fn chunk_string(str: String, chunk_size: usize) -> Vec<String> {
    str.chars()
        .fold(vec![String::new()], |mut chunk_vec, char| {
            if chunk_vec.last().unwrap().len() < chunk_size {
                let last = chunk_vec.last_mut().unwrap();
                last.push(char);
            } else {
                chunk_vec.push(char.to_string());
            }
            chunk_vec
        })
}
