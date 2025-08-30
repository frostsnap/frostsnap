#[macro_export]
macro_rules! init_display {
    (peripherals: $peripherals:ident, delay: $delay:expr) => {{
        use display_interface_spi::SPIInterface;
        use esp_hal::{
            gpio::{Level, Output},
            prelude::*,
            spi::{
                master::{Config as spiConfig, Spi},
                SpiMode,
            },
        };
        use mipidsi::{models::ST7789, options::ColorInversion};
        use $crate::display_init::NoCs;

        let spi = Spi::new_with_config(
            $peripherals.SPI2,
            spiConfig {
                frequency: 80.MHz(),
                mode: SpiMode::Mode2,
                ..spiConfig::default()
            },
        )
        .with_sck($peripherals.GPIO8)
        .with_mosi($peripherals.GPIO7);

        let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
        let di = SPIInterface::new(spi_device, Output::new($peripherals.GPIO9, Level::Low));

        let display = mipidsi::Builder::new(ST7789, di)
            .display_size(240, 280)
            .display_offset(0, 20) // 240*280 panel
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(Output::new($peripherals.GPIO6, Level::Low))
            .init($delay)
            .unwrap();

        display
    }};
}

// NoCs struct for SPI exclusive device
pub struct NoCs;

impl embedded_hal::digital::OutputPin for NoCs {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl embedded_hal::digital::ErrorType for NoCs {
    type Error = core::convert::Infallible;
}
