use core::panic::PanicInfo;

/// A fixed length string that doesn't require allocations.
///
/// Useful in panic handlers. If more text is written than it can fit it just silently overflows.
pub struct PanicBuffer<const N: usize> {
    buffer: [u8; N],
    buf_len: usize,
}

impl<const N: usize> Default for PanicBuffer<N> {
    fn default() -> Self {
        Self {
            buffer: [0u8; N],
            buf_len: 0,
        }
    }
}

impl<const N: usize> PanicBuffer<N> {
    pub fn as_str(&self) -> &str {
        match core::str::from_utf8(&self.buffer[..self.buf_len]) {
            Ok(string) => string,
            Err(_) => "failed to render panic",
        }
    }

    fn head(&mut self) -> &mut [u8] {
        &mut self.buffer[self.buf_len..]
    }
}

impl<const N: usize> core::fmt::Write for PanicBuffer<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let len = self.head().len().min(s.len());
        self.head()[..len].copy_from_slice(&s.as_bytes()[..len]);
        self.buf_len += len;
        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        let head = self.head();
        if !head.is_empty() {
            head[0] = c as u8;
            self.buf_len += 1;
        }
        Ok(())
    }
}

/// Shared panic handler implementation for device binaries
pub fn handle_panic(info: &PanicInfo) -> ! {
    use core::fmt::Write;
    use esp_hal::{
        delay::Delay,
        gpio::{Level, Output},
        peripherals::Peripherals,
    };

    // Get peripherals for panic display
    let peripherals = unsafe { Peripherals::steal() };
    let mut bl = Output::new(peripherals.GPIO1, Level::Low);
    let mut delay = Delay::new();

    let mut panic_buf = PanicBuffer::<512>::default();

    let _ = match info.location() {
        Some(location) => write!(
            &mut panic_buf,
            "{}:{} {}",
            location.file().split('/').next_back().unwrap_or(""),
            location.line(),
            info
        ),
        None => write!(&mut panic_buf, "{info}"),
    };

    // Initialize display for panic message
    macro_rules! init_display {
        (peripherals: $peripherals:ident, delay: $delay:expr) => {{
            use display_interface_spi::SPIInterface;
            use esp_hal::{
                gpio::{Level, Output},
                prelude::*,
                spi::{
                    master::{Config as SpiConfig, Spi},
                    SpiMode,
                },
            };
            use mipidsi::{models::ST7789, options::ColorInversion};

            struct NoCs;
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

            let spi = Spi::new_with_config(
                $peripherals.SPI2,
                SpiConfig {
                    frequency: 80.MHz(),
                    mode: SpiMode::Mode2,
                    ..SpiConfig::default()
                },
            )
            .with_sck($peripherals.GPIO8)
            .with_mosi($peripherals.GPIO7);

            let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
            let di = SPIInterface::new(spi_device, Output::new($peripherals.GPIO9, Level::Low));

            mipidsi::Builder::new(ST7789, di)
                .display_size(240, 280)
                .display_offset(0, 20)
                .invert_colors(ColorInversion::Inverted)
                .reset_pin(Output::new($peripherals.GPIO6, Level::Low))
                .init($delay)
                .unwrap()
        }};
    }

    let mut display = init_display!(peripherals: peripherals, delay: &mut delay);
    crate::graphics::error_print(&mut display, panic_buf.as_str());
    bl.set_high();
    loop {
        core::hint::spin_loop();
    }
}
