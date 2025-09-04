//! Production device binary

#![no_std]
#![no_main]

extern crate alloc;

use core::cell::RefCell;
use esp_hal::entry;
use esp_storage::FlashStorage;
use frostsnap_device::{
    esp32_run, peripherals::DevicePeripherals, resources::Resources,
};

#[entry]
fn main() -> ! {
    // Initialize heap
    esp_alloc::heap_allocator!(256 * 1024);

    // Initialize ESP32 hardware
    let mut peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = esp_hal::clock::CpuClock::max();
        config
    });

    // Initialize flash storage (must stay alive for partition references)
    let flash = RefCell::new(FlashStorage::new());

    // Initialize all device peripherals with initial RNG
    let device = DevicePeripherals::init(&mut peripherals);

    // Initialize resources (production mode - factory provisioning)
    let mut resources = Resources::init_production(device, &flash);

    // Run main event loop
    esp32_run::run(&mut resources);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
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

    let mut panic_buf = frostsnap_device::panic::PanicBuffer::<512>::default();

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
    frostsnap_device::graphics::error_print(&mut display, panic_buf.as_str());
    bl.set_high();
    loop {}
}
