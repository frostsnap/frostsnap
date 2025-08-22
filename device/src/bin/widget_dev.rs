#![no_std]
#![no_main]

extern crate alloc;
use cst816s::{TouchGesture, CST816S};
use esp_hal::{
    delay::Delay,
    entry,
    gpio::{Input, Level, Output, Pull},
    i2c::master::{Config as i2cConfig, I2c},
    prelude::*,
    timer::timg::TimerGroup,
};
use frostsnap_device::{
    debug_stats::create_debug_stats, init_display, touch_calibration::adjust_touch_point,
};

// Screen constants
const SCREEN_WIDTH: u32 = 240;
const SCREEN_HEIGHT: u32 = 280;

// Widget demo selection
const DEMO: &str = "bip39_entry";

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

    // Initialize the display using the macro
    let display_inner = init_display!(peripherals: peripherals, delay: &mut delay);

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
                .push_aligned(create_debug_stats(), Alignment::TopCenter);

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

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
