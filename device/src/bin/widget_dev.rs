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
use frostsnap_backup::bip39_words::BIP39_WORDS;
use frostsnap_device::{
    graphics::widgets::{
        DisplaySeedWords, EnterBip39ShareScreen,
        HoldToConfirmWidget, SizedBox, Widget,
    },
    touch_calibration::adjust_touch_point,
    Instant,
};
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

    // Macro to run a widget with all the boilerplate handling
    macro_rules! run_widget {
        ($widget:expr) => {{
            let mut widget = $widget;
            let mut last_touch: Option<(Point, u32)> = None;
            
            // Clear the screen with background color
            use frostsnap_device::graphics::palette::PALETTE;
            let _ = display.clear(PALETTE.background);

            // Main loop
            loop {
                // Get current time
                let current_time = Instant::from_ticks(timer.now().ticks());

                // Check for touch events
                if let Some(touch_event) = capsense.read_one_touch_event(true) {
                    // Only process if we have valid coordinates
                    if touch_event.x > 0 || touch_event.y > 0 {
                        // Apply touch calibration adjustments
                        let (adjusted_x, adjusted_y) =
                            adjust_touch_point(touch_event.x as i32, touch_event.y as i32);
                        let touch_point = Point::new(adjusted_x, adjusted_y);
                        let lift_up = touch_event.action == 1;
                        let gesture = touch_event.gesture;

                        // Handle vertical drag for widgets that support it
                        match gesture {
                            TouchGesture::SlideUp | TouchGesture::SlideDown => {
                                widget.handle_vertical_drag(
                                    last_touch.map(|(_, y)| y),
                                    adjusted_y as u32,
                                );
                            }
                            _ => {
                                // Handle regular touches
                                widget.handle_touch(touch_point, current_time, lift_up);
                            }
                        }

                        // Store last touch for drag calculations
                        if !lift_up {
                            last_touch = Some((touch_point, adjusted_y as u32));
                        } else {
                            last_touch = None;
                        }
                    }
                }

                // Draw the widget
                let _ = widget.draw(&mut display, current_time);
            }
        }};
    }

    // Configuration: Change this to select which widget to display
    let show = "confirm_touch";

    let screen_size = Size::new(240, 280);

    // Create and run the selected widget
    match show {
        "bip39_entry" => {
            // BIP39 entry screen
            run_widget!(EnterBip39ShareScreen::new(screen_size));
        }
        "bip39_view" => {
            // Display seed words - using random indices
            const TEST_WORDS: [&'static str; 25] = [
                BIP39_WORDS[1337],  // owner
                BIP39_WORDS[432],   // deny
                BIP39_WORDS[1789],  // survey
                BIP39_WORDS[923],   // journey
                BIP39_WORDS[567],   // embark
                BIP39_WORDS[1456],  // recall
                BIP39_WORDS[234],   // churn
                BIP39_WORDS[1678],  // spawn
                BIP39_WORDS[890],   // invest
                BIP39_WORDS[345],   // crater
                BIP39_WORDS[1234],  // neutral
                BIP39_WORDS[678],   // fiscal
                BIP39_WORDS[1890],  // thumb
                BIP39_WORDS[456],   // diamond
                BIP39_WORDS[1567],  // robot
                BIP39_WORDS[789],   // guitar
                BIP39_WORDS[1345],  // oyster
                BIP39_WORDS[123],   // badge
                BIP39_WORDS[1789],  // survey
                BIP39_WORDS[567],   // embark
                BIP39_WORDS[1012],  // lizard
                BIP39_WORDS[1456],  // recall
                BIP39_WORDS[789],   // guitar
                BIP39_WORDS[1678],  // spawn
                BIP39_WORDS[234],   // churn
            ];
            let share_index = 42;
            run_widget!(DisplaySeedWords::new(screen_size, TEST_WORDS, share_index));
        }
        "confirm_touch" => {
            // Hold to confirm with empty widget (1.5 seconds to confirm)
            let sized_box = SizedBox::new(screen_size);
            let mut hold_to_confirm = HoldToConfirmWidget::new(sized_box, 1500.0);
            hold_to_confirm.enable();
            run_widget!(hold_to_confirm);
        }
        _ => {
            // Default to BIP39 entry
            run_widget!(EnterBip39ShareScreen::new(screen_size));
        }
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
