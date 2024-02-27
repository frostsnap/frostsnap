// Air101 ST7735 driver
use display_interface_spi::SPIInterfaceNoCS;
use embedded_graphics::{
    mono_font::{
        ascii::{FONT_10X20, FONT_5X8, FONT_7X14},
        MonoTextStyle,
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
};
use embedded_graphics_framebuf::FrameBuf;
use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::{TextBoxStyle, TextBoxStyleBuilder},
    TextBox,
};
use hal::{
    clock::Clocks,
    gpio::{AnyPin, Output, OutputPin, PushPull},
    peripheral::Peripheral,
    prelude::*,
    spi::{
        master::{Instance, Spi},
        FullDuplexMode, SpiMode,
    },
    Delay,
};
use mipidsi::ColorInversion;
use mipidsi::{models::ST7735s, Display};

pub type SpiInterface<'d, SPI> =
    SPIInterfaceNoCS<Spi<'d, SPI, FullDuplexMode>, AnyPin<Output<PushPull>>>;

pub struct ST7735<'d, SPI>
where
    SPI: Instance,
{
    pub display: Display<SpiInterface<'d, SPI>, ST7735s, AnyPin<Output<PushPull>>>,
    character_style: MonoTextStyle<'d, Rgb565>,
    textbox_style: TextBoxStyle,
    framebuf: FrameBuf<Rgb565, [Rgb565; 12800]>,
}

impl<'d, SPI> ST7735<'d, SPI>
where
    SPI: Instance,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new<SCK: OutputPin, MOSI: OutputPin>(
        dc: AnyPin<Output<PushPull>>,
        rst: AnyPin<Output<PushPull>>,
        spi: impl Peripheral<P = SPI> + 'd,
        sck: impl Peripheral<P = SCK> + 'd,
        mosi: impl Peripheral<P = MOSI> + 'd,
        clocks: &Clocks,
        framebuf: FrameBuf<Rgb565, [Rgb565; 12800]>,
    ) -> Result<Self, mipidsi::Error> {
        let spi = Spi::new(spi, 16u32.MHz(), SpiMode::Mode0, clocks)
            .with_sck(sck)
            .with_mosi(mosi);

        let di = SPIInterfaceNoCS::new(spi, dc);
        let mut delay = Delay::new(clocks);

        // default values are for the air101-r225
        const OFFSET_HANDLER: (u16, u16) = {
            let _val = (1, 26);
            #[cfg(feature = "air101-r2223")]
            let _val = (0, 24);
            _val
        };
        const INVERT_COLORS: ColorInversion = {
            let _val = ColorInversion::Inverted;
            #[cfg(feature = "air101-r2223")]
            let _val = ColorInversion::Normal;
            _val
        };
        let mut display = mipidsi::Builder::st7735s(di)
            .with_display_size(80, 160)
            .with_window_offset_handler(|_| OFFSET_HANDLER)
            .with_invert_colors(INVERT_COLORS)
            .with_color_order(mipidsi::options::ColorOrder::Bgr)
            .init(&mut delay, Some(rst))
            .expect("infallible");

        display.set_orientation(mipidsi::options::Orientation::Landscape(true))?;

        let character_style = MonoTextStyle::new(&FONT_7X14, Rgb565::WHITE);
        let textbox_style = TextBoxStyleBuilder::new()
            // .alignment(HorizontalAlignment::Center)
            // .vertical_alignment(VerticalAlignment::Middle)
            // .line_height(LineHeight::Pixels(16))
            .build();

        let mut _self = Self {
            display,
            character_style,
            textbox_style,
            framebuf,
        };

        _self.clear(Rgb565::BLACK);
        _self.flush()?;

        Ok(_self)
    }

    pub fn flush(&mut self) -> Result<(), mipidsi::Error> {
        let area = Rectangle::new(Point::new(0, 0), self.framebuf.size());
        self.display.fill_contiguous(&area, self.framebuf.data)?;
        Ok(())
    }

    pub fn error_print(&mut self, error: impl AsRef<str>) {
        let header_area = Rectangle::new(Point::zero(), Size::new(160, 10));
        let _ = header_area
            .into_styled(PrimitiveStyleBuilder::new().fill_color(Rgb565::RED).build())
            .draw(&mut self.framebuf);

        let header_charstyle = MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE);
        let _ = TextBox::with_textbox_style(
            "ERROR",
            Rectangle::new(Point::new(1, 1), Size::new(80, 10)),
            header_charstyle,
            self.textbox_style,
        )
        .draw(&mut self.framebuf);

        Line::new(Point::new(0, 10), Point::new(160, 10))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_GRAY, 1))
            .draw(&mut self.framebuf)
            .unwrap();

        let _ = Rectangle::new(Point::new(0, 11), Size::new(160, 80))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf);

        let character_style = MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE);
        let _ = TextBox::with_textbox_style(
            error.as_ref(),
            Rectangle::new(Point::new(1, 11), Size::new(160, 80)),
            character_style,
            self.textbox_style,
        )
        .draw(&mut self.framebuf);

        let _ = self.flush();
    }

    pub fn splash_screen(&mut self, percent: f32) -> Result<(), mipidsi::Error> {
        let incomplete = 1.0 - percent;
        self.clear(Rgb565::BLACK);

        TextBox::with_textbox_style(
            "Frost",
            Rectangle::new(
                Point::new((-30.0 * incomplete) as i32, 0),
                Size::new(80, 80),
            ),
            MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Middle)
                // .line_height(LineHeight::Pixels(16))
                .build(),
        )
        .draw(&mut self.framebuf)
        .unwrap();
        TextBox::with_textbox_style(
            "Snap",
            Rectangle::new(
                Point::new(80 + (30.0 * incomplete) as i32, 0),
                Size::new(80, 80),
            ),
            MonoTextStyle::new(&FONT_10X20, Rgb565::CYAN),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Left)
                .vertical_alignment(VerticalAlignment::Middle)
                // .line_height(LineHeight::Pixels(16))
                .build(),
        )
        .draw(&mut self.framebuf)
        .unwrap();

        self.flush()
    }

    pub fn set_top_left_square(&mut self, color: Rgb565) {
        Rectangle::new(Point::zero(), Size::new_equal(10))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(Rgb565::new(4, 8, 17))
                    .stroke_width(1)
                    .fill_color(color)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();
    }

    pub fn header(&mut self, device_label: impl AsRef<str>) {
        Rectangle::new(Point::zero(), Size::new(160, 10))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::new(4, 8, 17))
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        let header_charstyle = MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE);
        let textbox_style = TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .build();
        TextBox::with_textbox_style(
            device_label.as_ref(),
            Rectangle::new(Point::new(10, 1), Size::new(140, 10)),
            header_charstyle,
            textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();

        Line::new(Point::new(0, 10), Point::new(160, 10))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_GRAY, 1))
            .draw(&mut self.framebuf)
            .unwrap();
    }

    pub fn print(&mut self, str: impl AsRef<str>) {
        Rectangle::new(Point::new(0, 11), Size::new(160, 80))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        let _overflow = TextBox::with_textbox_style(
            str.as_ref(),
            Rectangle::new(Point::new(1, 11), Size::new(160, 80)),
            self.character_style,
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();
    }

    pub fn confirm_bar(&mut self, percent: f32) {
        let y = 78;
        if percent == 0.0 {
            Line::new(Point::new(0, y), Point::new(160, y))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(7, 14, 7), 2))
                .draw(&mut self.framebuf)
                .unwrap();
        } else if percent == 100.0 {
            Line::new(Point::new(0, y), Point::new(160, y))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
                .draw(&mut self.framebuf)
                .unwrap();
        } else {
            // skips framebuffer to directly draw line onto display
            Line::new(Point::new(0, y), Point::new((160.0 * percent) as i32, y))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
                .draw(&mut self.display)
                .unwrap();
        }
    }

    pub fn clear(&mut self, c: Rgb565) {
        Rectangle::new(Point::new(0, 0), Size::new(160, 80))
            .into_styled(PrimitiveStyleBuilder::new().fill_color(c).build())
            .draw(&mut self.framebuf)
            .unwrap();
    }

    pub fn set_mem_debug(&mut self, used: usize, free: usize) {
        Rectangle::new(Point::new(80, 60), Size::new(80, 20))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        TextBox::with_textbox_style(
            &format!("{}/{}", used, free),
            Rectangle::new(Point::new(80, 60), Size::new(80, 20)),
            MonoTextStyle::new(&FONT_7X14, Rgb565::GREEN),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Right)
                .build(),
        )
        .draw(&mut self.framebuf)
        .unwrap();
    }
}
