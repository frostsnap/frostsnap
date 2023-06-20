// SSD1306 72x40 OLED driver

use display_interface::DisplayError;
use embedded_graphics::{
    mono_font::{
        ascii::{FONT_6X10, FONT_9X18_BOLD},
        MonoTextStyle,
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::*,
};
use embedded_text::{
    alignment::HorizontalAlignment,
    style::{TextBoxStyle, TextBoxStyleBuilder},
    TextBox,
};
use esp32c3_hal::{
    clock::Clocks,
    gpio::{InputPin, OutputPin},
    i2c::{Instance, I2C},
    peripheral::Peripheral,
    system::PeripheralClockControl,
};
use fugit::HertzU32;
use ssd1306::{mode::BufferedGraphicsMode, prelude::*, I2CDisplayInterface, Ssd1306};

pub struct SSD1306<'d, T>
where
    // DI: WriteOnlyDataCommand,
    // SIZE: DisplaySize,
{
    pub display:
        Ssd1306<I2CInterface<I2C<'d, T>>, DisplaySize72x40, BufferedGraphicsMode<DisplaySize72x40>>,
    character_style: MonoTextStyle<'d, BinaryColor>,
    textbox_style: TextBoxStyle,
}

impl<'d, T> SSD1306<'d, T>
where
    // DI: WriteOnlyDataCommand,
    // SIZE: DisplaySize,
    T: Instance,
{
    pub fn new<SDA: OutputPin + InputPin, SCL: OutputPin + InputPin>(
        i2c: impl Peripheral<P = T> + 'd,
        sda: impl Peripheral<P = SDA> + 'd,
        scl: impl Peripheral<P = SCL> + 'd,
        frequency: HertzU32,
        peripheral_clock_control: &mut PeripheralClockControl,
        clocks: &Clocks,
    ) -> Result<Self, DisplayError> {
        let i2c = I2C::new(i2c, sda, scl, frequency, peripheral_clock_control, clocks);

        // Initialize display
        let interface = I2CDisplayInterface::new(i2c);
        let mut display = Ssd1306::new(interface, DisplaySize72x40, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();

        display.init().unwrap();
        display.clear();
        display.flush().unwrap();

        let character_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let textbox_style = TextBoxStyleBuilder::new()
            // .alignment(HorizontalAlignment::Center)
            // .vertical_alignment(VerticalAlignment::Middle)
            // .line_height(LineHeight::Pixels(16))
            .build();
        Ok(Self {
            display,
            character_style,
            textbox_style,
        })
    }

    pub fn flush(&mut self) -> Result<(), DisplayError> {
        self.display.flush().unwrap();
        Ok(())
    }

    pub fn print(&mut self, str: impl AsRef<str>) -> Result<(), DisplayError> {
        self.display.clear();
        TextBox::with_textbox_style(
            str.as_ref(),
            Rectangle::new(Point::new(0, 0), Size::new(72, 40)),
            self.character_style,
            self.textbox_style,
        )
        .draw(&mut self.display)
        .unwrap();

        self.display.flush().unwrap();

        Ok(())
    }

    pub fn print_header(&mut self, str: impl AsRef<str>) -> Result<(), DisplayError> {
        self.display.clear();
        TextBox::with_textbox_style(
            str.as_ref(),
            Rectangle::new(Point::new(0, 0), Size::new(72, 40)),
            MonoTextStyle::new(&FONT_9X18_BOLD, BinaryColor::On),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Center)
                // .vertical_alignment(VerticalAlignment::Middle)
                // .line_height(LineHeight::Pixels(16))
                .build(),
        )
        .draw(&mut self.display)
        .unwrap();

        self.display.flush().unwrap();

        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), DisplayError> {
        self.display.clear();
        Ok(())
    }
}

// for x in 0..72 {
//     Rectangle::new(Point::new(x, x / 2), size)
//         .into_styled(style)
//         .draw(&mut display)
//         .unwrap();
//     display.flush().unwrap();
//     sleep(Duration::from_millis(50));

//     display.clear();
// }
