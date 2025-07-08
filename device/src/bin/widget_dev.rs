#![no_std]
#![no_main]

extern crate alloc;
use cst816s::{TouchGesture, CST816S};
use display_interface_spi::SPIInterface;
use embedded_graphics::prelude::*;
use esp_hal::{
    delay::Delay,
    entry,
    gpio::{Input, Level, Output, Pull},
    i2c::master::{Config as i2cConfig, I2c},
    peripherals::Peripherals,
    prelude::*,
    spi::{
        master::{Config as spiConfig, Spi},
        SpiMode,
    },
    timer::timg::TimerGroup,
};
use frostsnap_device::{
    graphics::widgets::{DisplaySeedWords, MemoryDebugWidget}, 
    touch_calibration::adjust_touch_point, 
    Instant,
};
use frostsnap_backup::bip39_words::BIP39_WORDS;
use mipidsi::{models::ST7789, options::ColorInversion};

#[entry]
fn main() -> ! {
    esp_alloc::heap_allocator!(256 * 1024);
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::max();
        config
    });

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timer = timg0.timer0;

    let mut delay = Delay::new();

    // Initialize backlight
    let mut bl = Output::new(peripherals.GPIO1, Level::Low);

    // Initialize SPI for the display
    let spi = Spi::new_with_config(
        peripherals.SPI2,
        spiConfig {
            frequency: 80.MHz(),
            mode: SpiMode::Mode2,
            ..spiConfig::default()
        },
    )
    .with_sck(peripherals.GPIO8)
    .with_mosi(peripherals.GPIO7);
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
    let di = SPIInterface::new(spi_device, Output::new(peripherals.GPIO9, Level::Low));
    let mut display = mipidsi::Builder::new(ST7789, di)
        .display_size(240, 280)
        .display_offset(0, 20) // 240x280 panel
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(peripherals.GPIO6, Level::Low))
        .init(&mut delay)
        .unwrap();

    // Initialize I2C for CST816S touch controller
    let i2c = I2c::new(
        peripherals.I2C0,
        i2cConfig {
            frequency: 400u32.kHz(),
            ..i2cConfig::default()
        },
    )
    .with_sda(peripherals.GPIO4)
    .with_scl(peripherals.GPIO5);
    let mut capsense = CST816S::new(
        i2c,
        Input::new(peripherals.GPIO2, Pull::Down),
        Output::new(peripherals.GPIO3, Level::Low),
    );
    capsense.setup(&mut delay).unwrap();

    // Turn on backlight
    bl.set_high();

    // Test BIP39 words - using indexes into BIP39_WORDS
    // Using first 25 words: ABANDON(0) through ADAPT(25)
    const TEST_WORDS: [&'static str; 25] = [
        BIP39_WORDS[0],   // ABANDON
        BIP39_WORDS[1],   // ABILITY
        BIP39_WORDS[2],   // ABLE
        BIP39_WORDS[3],   // ABOUT
        BIP39_WORDS[4],   // ABOVE
        BIP39_WORDS[5],   // ABSENT
        BIP39_WORDS[6],   // ABSORB
        BIP39_WORDS[7],   // ABSTRACT
        BIP39_WORDS[8],   // ABSURD
        BIP39_WORDS[9],   // ABUSE
        BIP39_WORDS[10],  // ACCESS
        BIP39_WORDS[11],  // ACCIDENT
        BIP39_WORDS[12],  // ACCOUNT
        BIP39_WORDS[13],  // ACCUSE
        BIP39_WORDS[14],  // ACHIEVE
        BIP39_WORDS[15],  // ACID
        BIP39_WORDS[16],  // ACOUSTIC
        BIP39_WORDS[17],  // ACQUIRE
        BIP39_WORDS[18],  // ACROSS
        BIP39_WORDS[19],  // ACT
        BIP39_WORDS[20],  // ACTION
        BIP39_WORDS[21],  // ACTOR
        BIP39_WORDS[22],  // ACTRESS
        BIP39_WORDS[23],  // ACTUAL
        BIP39_WORDS[24],  // ADAPT
    ];
    
    // Initialize the DisplaySeedWords widget
    let screen_size = Size::new(240, 280);
    let share_index = 42; // Example share index
    let mut display_widget = DisplaySeedWords::new(screen_size, TEST_WORDS, share_index);
    
    // Initialize memory debug widget
    let mut mem_debug = MemoryDebugWidget::new(240, 280);

    let mut last_touch = None;

    // Main loop
    loop {
        // Get current time
        let current_time = Instant::from_ticks(timer.now().ticks());

        // Check for touch events
        if let Some(touch_event) = capsense.read_one_touch_event(true) {
            // Apply touch calibration adjustments
            let (adjusted_x, adjusted_y) =
                adjust_touch_point(touch_event.x as i32, touch_event.y as i32);
            let touch_point = Point::new(adjusted_x, adjusted_y);
            let lift_up = touch_event.action == 1;
            let gesture = touch_event.gesture;

            // Store last touch for drag calculations
            let prev_touch = last_touch.take();
            if !lift_up {
                last_touch = Some((touch_point, adjusted_y as u32));
            }

            // Handle touches for the display widget
            display_widget.handle_touch(touch_point, current_time, lift_up);
        }

        // Draw the display widget
        display_widget.draw(&mut display, current_time);
        
        // Update and draw memory debug info
        mem_debug.update(esp_alloc::HEAP.used(), esp_alloc::HEAP.free());
        mem_debug.draw(&mut display, current_time).unwrap();
    }
}

/// Dummy CS pin for the display
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

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    let peripherals = unsafe { Peripherals::steal() };

    let mut bl = Output::new(peripherals.GPIO1, Level::Low);

    let mut delay = Delay::new();
    let mut panic_buf = frostsnap_device::panic::PanicBuffer::<512>::default();

    let _ = match info.location() {
        Some(location) => write!(
            &mut panic_buf,
            "{}:{} {}",
            location.file().split('/').last().unwrap_or(""),
            location.line(),
            info
        ),
        None => write!(&mut panic_buf, "{}", info),
    };

    let spi = Spi::new_with_config(
        peripherals.SPI2,
        spiConfig {
            frequency: 80.MHz(),
            mode: SpiMode::Mode2,
            ..spiConfig::default()
        },
    )
    .with_sck(peripherals.GPIO8)
    .with_mosi(peripherals.GPIO7);
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);

    let di = SPIInterface::new(spi_device, Output::new(peripherals.GPIO9, Level::Low));
    let mut display = mipidsi::Builder::new(ST7789, di)
        .display_size(240, 280)
        .display_offset(0, 20) // 240*280 panel
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(peripherals.GPIO6, Level::Low))
        .init(&mut delay)
        .unwrap();
    frostsnap_device::graphics::error_print(&mut display, panic_buf.as_str());
    bl.set_high();

    loop {}
}
