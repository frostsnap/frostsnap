#![no_std]
#![no_main]

extern crate alloc;
use esp_hal::{
    delay::Delay,
    entry,
    gpio::{Io, Level, Output},
    i2c::master::{Config as i2cConfig, I2c},
    prelude::*,
    timer::timg::TimerGroup,
};
use frostsnap_cst816s::{interrupt::TouchReceiver, CST816S};
use frostsnap_device::{init_display, touch_handler};
use frostsnap_widgets::debug::{EnabledDebug, OverlayDebug};

// Screen constants
const SCREEN_WIDTH: u32 = 240;
const SCREEN_HEIGHT: u32 = 280;

// Widget demo selection
const DEMO: &str = "hold_confirm";

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

    // Set up GPIO for interrupt handling
    let mut io = Io::new(peripherals.IO_MUX);

    // Initialize backlight
    let mut bl = Output::new(peripherals.GPIO1, Level::Low);

    // Initialize the display using the macro
    let display_inner = init_display!(peripherals: peripherals, delay: &mut delay);

    let mut display = frostsnap_widgets::SuperDrawTarget::new(
        display_inner,
        frostsnap_widgets::palette::PALETTE.background,
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

    let mut capsense = CST816S::new_esp32(i2c, peripherals.GPIO2, peripherals.GPIO3);
    capsense.setup(&mut delay).unwrap();

    // Register the capsense instance with the interrupt handler
    let mut touch_receiver: TouchReceiver =
        frostsnap_cst816s::interrupt::register(capsense, &mut io);

    // Turn on backlight
    bl.set_high();

    let _screen_size = Size::new(SCREEN_WIDTH, SCREEN_HEIGHT);

    // Macro to run a widget with all the hardware peripherals
    macro_rules! run_widget {
        ($widget:expr) => {{
            // Create the widget with debug overlay
            let debug_config = EnabledDebug {
                logs: cfg!(feature = "debug_log"),
                memory: cfg!(feature = "debug_mem"),
                fps: cfg!(feature = "debug_fps"),
            };
            let mut widget_with_debug = OverlayDebug::new($widget, debug_config);

            // Set constraints
            widget_with_debug.set_constraints(Size::new(240, 280));

            let mut last_touch: Option<Point> = None;
            let mut current_widget_index = 0usize;

            // Track last redraw time
            let mut last_redraw_time = timer.now();

            // Clear the screen with background color
            let _ = display.clear(PALETTE.background);

            // Main loop
            loop {
                let now = timer.now();
                let now_ms =
                    frostsnap_widgets::Instant::from_millis(now.duration_since_epoch().to_millis());

                // Process all pending touch events
                touch_handler::process_all_touch_events(
                    &mut touch_receiver,
                    &mut widget_with_debug,
                    &mut last_touch,
                    &mut current_widget_index,
                    now_ms,
                );

                // Only redraw if at least 10ms has passed since last redraw
                let elapsed_ms = (now - last_redraw_time).to_millis();
                if elapsed_ms >= 5 {
                    // Draw the UI stack (includes debug stats overlay)
                    let _ = widget_with_debug.draw(&mut display, now_ms);

                    // Update last redraw time
                    last_redraw_time = now;
                }
            }
        }};
    }

    // Use the demo_widget! macro from frostsnap_widgets
    frostsnap_widgets::demo_widget!(DEMO, screen_size, run_widget);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
