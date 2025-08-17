#![no_std]
#![no_main]

extern crate alloc;
use cst816s::{TouchGesture, CST816S};
use display_interface_spi::SPIInterface;
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
use frostsnap_device::debug_stats::create_debug_stats;
use frostsnap_device::touch_calibration::adjust_touch_point;
use frostsnap_embedded_widgets::{DynWidget, Stack, StackAlignment};
use mipidsi::{models::ST7789, options::ColorInversion};

// Screen constants
const SCREEN_WIDTH: u32 = 240;
const SCREEN_HEIGHT: u32 = 280;
const SCREEN_OFFSET_Y: u16 = 20; // ST7789 Y offset for 240x280 panel

// Widget demo selection
const DEMO: &str = "sign_prompt";

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
    let display_inner = mipidsi::Builder::new(ST7789, di)
        .display_size(SCREEN_WIDTH as u16, SCREEN_HEIGHT as u16)
        .display_offset(0, SCREEN_OFFSET_Y)
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(peripherals.GPIO6, Level::Low))
        .init(&mut delay)
        .unwrap();

    let mut display = frostsnap_embedded_widgets::SuperDrawTarget::new(
        display_inner,
        frostsnap_embedded_widgets::palette::PALETTE.background,
    );

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

    let screen_size = Size::new(SCREEN_WIDTH, SCREEN_HEIGHT);

    // Macro to run a widget with all the hardware peripherals
    macro_rules! run_widget {
        ($widget:expr) => {{
            let widget = $widget;

            // Create UI stack with widget and debug stats overlay
            let mut ui_stack = Stack::builder()
                .push(widget)
                .push_aligned(create_debug_stats(), StackAlignment::TopLeft);

            // Set constraints on the stack
            ui_stack.set_constraints(Size::new(240, 280));
            let mut last_touch: Option<(Point, u32)> = None;

            // Track last redraw time
            let mut last_redraw_time = timer.now();

            // Clear the screen with background color
            let _ = display.clear(PALETTE.background);

            // Main loop
            loop {
                // Get current time
                let current_time = timer.now();

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

                        let is_vertical_drag =
                            matches!(gesture, TouchGesture::SlideUp | TouchGesture::SlideDown);

                        // Handle vertical drag for widgets that support it
                        if is_vertical_drag {
                            ui_stack.handle_vertical_drag(
                                last_touch.map(|(_, y)| y),
                                adjusted_y as u32,
                                lift_up,
                            );
                        }

                        if !is_vertical_drag || lift_up {
                            // Always handle touch events (for both press and release)
                            // This is important so that lift_up is processed after drag
                            ui_stack.handle_touch(
                                touch_point,
                                frostsnap_embedded_widgets::Instant::from_millis(
                                    current_time.duration_since_epoch().to_millis(),
                                ),
                                lift_up,
                            );
                        }
                        // Store last touch for drag calculations
                        if lift_up {
                            last_touch = None;
                        } else {
                            last_touch = Some((touch_point, adjusted_y as u32));
                        }
                    }
                }

                // Only redraw if at least 10ms has passed since last redraw
                let elapsed_ms = (current_time - last_redraw_time).to_millis();
                if elapsed_ms >= 5 {
                    // Draw the UI stack (includes debug stats overlay)
                    let _ = ui_stack.draw(
                        &mut display,
                        frostsnap_embedded_widgets::Instant::from_millis(
                            current_time.duration_since_epoch().to_millis(),
                        ),
                    );

                    // Update last redraw time
                    last_redraw_time = current_time;
                }
            }
        }};
    }

    // Use the demo_widget! macro from frostsnap_embedded_widgets
    frostsnap_embedded_widgets::demo_widget!(DEMO, screen_size, run_widget);
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
            location.file().split('/').next_back().unwrap_or(""),
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
        .display_size(SCREEN_WIDTH as u16, SCREEN_HEIGHT as u16)
        .display_offset(0, SCREEN_OFFSET_Y)
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(peripherals.GPIO6, Level::Low))
        .init(&mut delay)
        .unwrap();
    frostsnap_device::panic::error_print(&mut display, panic_buf.as_str());
    bl.set_high();

    loop {}
}
