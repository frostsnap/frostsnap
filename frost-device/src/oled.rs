use anyhow::{bail, Error, Result};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
};
use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::{HeightMode, TextBoxStyle, TextBoxStyleBuilder},
    TextBox,
};
use esp_idf_hal::{
    gpio::{InputPin, OutputPin},
    i2c::*,
    peripheral::Peripheral,
    units::*,
};
use ssd1306::{mode::BufferedGraphicsMode, prelude::*, I2CDisplayInterface, Ssd1306};

pub struct Oled<'d> {
    pub display: Ssd1306<
        I2CInterface<I2cDriver<'d>>,
        DisplaySize72x40,
        BufferedGraphicsMode<DisplaySize72x40>,
    >,
    character_style: MonoTextStyle<'d, BinaryColor>,
    textbox_style: TextBoxStyle,
}

impl<'d> Oled<'d> {
    pub fn new<I2C: I2c>(
        i2c: impl Peripheral<P = I2C> + 'd,
        sda: impl Peripheral<P = impl InputPin + OutputPin> + 'd,
        scl: impl Peripheral<P = impl InputPin + OutputPin> + 'd,
        rotation: DisplayRotation,
    ) -> Result<Self, Error> {
        let config = I2cConfig::new().baudrate(400.kHz().into());
        let i2c = I2cDriver::new(i2c, sda, scl, &config)?;
        let interface = I2CDisplayInterface::new(i2c);
        let mut display =
            Ssd1306::new(interface, DisplaySize72x40, rotation).into_buffered_graphics_mode();
        display.init().unwrap();
        display.clear();
        display.flush().unwrap();
        let character_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let textbox_style = TextBoxStyleBuilder::new()
            // .alignment(HorizontalAlignment::Center)
            // .vertical_alignment(VerticalAlignment::Middle)
            .build();
        Ok(Self {
            display,
            character_style,
            textbox_style,
        })
    }

    pub fn print(&mut self, str: impl AsRef<str>) -> Result<()> {
        TextBox::with_textbox_style(
            str.as_ref(),
            self.display.bounding_box(),
            self.character_style,
            self.textbox_style,
        )
        .draw(&mut self.display)
        .unwrap();
        self.display.flush().unwrap();
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
