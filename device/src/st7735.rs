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
use esp32c3_hal::{
    clock::Clocks,
    gpio::{AnyPin, InputPin, Output, OutputPin, PushPull},
    peripheral::Peripheral,
    prelude::*,
    spi::{FullDuplexMode, Instance, Spi, SpiMode},
    system::PeripheralClockControl,
    Delay,
};
use mipidsi::ColorInversion;
use mipidsi::{models::ST7735s, Display, Error};

pub type SpiInterface<'d, SPI> =
    SPIInterfaceNoCS<Spi<'d, SPI, FullDuplexMode>, AnyPin<Output<PushPull>>>;

pub struct ST7735<'d, SPI>
where
    SPI: Instance,
{
    // pub bl: &'d mut GpioPin<Output<PushPull>, RA, IRA, InputOutputPinType, Gpio11Signals, 11>,
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
    pub fn new<SCK: OutputPin, MOSI: OutputPin, MISO: InputPin, CS: OutputPin>(
        // bl: &'d mut GpioPin<Output<PushPull>, RA, IRA, InputOutputPinType, Gpio11Signals, 11>,
        dc: AnyPin<Output<PushPull>>,
        rst: AnyPin<Output<PushPull>>,
        spi: impl Peripheral<P = SPI> + 'd,
        sck: impl Peripheral<P = SCK> + 'd,
        cs: impl Peripheral<P = CS> + 'd,
        mosi: impl Peripheral<P = MOSI> + 'd,
        miso: impl Peripheral<P = MISO> + 'd,
        peripheral_clock_control: &mut PeripheralClockControl,
        clocks: &Clocks,
        framebuf: FrameBuf<Rgb565, [Rgb565; 12800]>,
    ) -> Result<Self, Error> {
        let spi = Spi::new(
            spi,
            sck,
            mosi,
            miso,
            cs,
            16u32.MHz(),
            SpiMode::Mode0,
            peripheral_clock_control,
            clocks,
        );

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
            .unwrap();

        display
            .set_orientation(mipidsi::options::Orientation::Landscape(true))
            .unwrap();

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

        _self.clear(Rgb565::BLACK).unwrap();
        _self.flush().unwrap();

        Ok(_self)
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        let area = Rectangle::new(Point::new(0, 0), self.framebuf.size());
        self.display
            .fill_contiguous(&area, self.framebuf.data)
            .unwrap();
        Ok(())
    }

    pub fn error_print(&mut self, str: impl AsRef<str>) -> Result<(), Error> {
        let header_area = Rectangle::new(Point::zero(), Size::new(160, 10));
        header_area
            .into_styled(PrimitiveStyleBuilder::new().fill_color(Rgb565::RED).build())
            .draw(&mut self.framebuf)
            .unwrap();

        let header_charstyle = MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE);
        TextBox::with_textbox_style(
            "ERROR",
            Rectangle::new(Point::new(1, 1), Size::new(80, 10)),
            header_charstyle,
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();

        Line::new(Point::new(0, 10), Point::new(160, 10))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_GRAY, 1))
            .draw(&mut self.framebuf)
            .unwrap();

        Rectangle::new(Point::new(0, 11), Size::new(160, 80))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        let character_style = MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE);
        TextBox::with_textbox_style(
            str.as_ref(),
            Rectangle::new(Point::new(1, 11), Size::new(160, 80)),
            character_style,
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();

        self.flush().unwrap();

        Ok(())
    }

    pub fn splash_screen(&mut self, percent: f32) -> Result<(), Error> {
        let incomplete = 1.0 - percent;
        self.clear(Rgb565::BLACK).unwrap();

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

        self.flush().unwrap();
        Ok(())
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

        self.flush().unwrap();
    }

    pub fn header(&mut self, device_label: impl AsRef<str>) -> Result<(), Error> {
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

        Ok(())
    }

    pub fn print(&mut self, str: impl AsRef<str>) -> Result<(), Error> {
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

        self.flush().unwrap();

        Ok(())
    }

    pub fn print_header(&mut self, str: impl AsRef<str>) -> Result<(), Error> {
        Rectangle::new(Point::new(0, 11), Size::new(160, 80))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(&mut self.framebuf)
            .unwrap();

        TextBox::with_textbox_style(
            str.as_ref(),
            Rectangle::new(Point::new(1, 11), Size::new(160, 80)),
            self.character_style,
            self.textbox_style,
        )
        .draw(&mut self.framebuf)
        .unwrap();

        self.flush().unwrap();

        Ok(())
    }

    pub fn confirm_bar(&mut self, percent: f32) -> Result<(), Error> {
        let y = 78;
        if percent == 0.0 {
            Line::new(Point::new(0, y), Point::new(160, y))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(7, 14, 7), 2))
                .draw(&mut self.framebuf)
                .unwrap();
        } else {
            Line::new(Point::new(0, y), Point::new((160.0 * percent) as i32, y))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
                .draw(&mut self.framebuf)
                .unwrap();
        }

        self.flush().unwrap();

        Ok(())
    }

    pub fn clear(&mut self, c: Rgb565) -> Result<(), Error> {
        Rectangle::new(Point::new(0, 0), Size::new(160, 80))
            .into_styled(PrimitiveStyleBuilder::new().fill_color(c).build())
            .draw(&mut self.framebuf)
            .unwrap();

        Ok(())
    }
}
