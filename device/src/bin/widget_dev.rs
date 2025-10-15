#![no_std]
#![no_main]

extern crate alloc;
use esp_hal::{entry, timer::Timer as _};
use frostsnap_device::{peripherals::DevicePeripherals, touch_handler, DISPLAY_REFRESH_MS};
use frostsnap_widgets::debug::{EnabledDebug, OverlayDebug};

// Widget demo selection
const DEMO: &str = "sign_prompt";

#[entry]
fn main() -> ! {
    esp_alloc::heap_allocator!(256 * 1024);

    // Initialize ESP32 hardware
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = esp_hal::clock::CpuClock::max();
        config
    });

    // Initialize all device peripherals
    let device = DevicePeripherals::init(peripherals);

    // Check if the device needs provisioning
    if device.needs_factory_provisioning() {
        // Run dev provisioning - this will reset the device
        frostsnap_device::factory::run_dev_provisioning(device);
    } else {
        // Device is already provisioned - proceed with widget testing
        // Extract the components we need from DevicePeripherals
        let DevicePeripherals {
            display,
            mut touch_receiver,
            timer,
            ..
        } = *device;

        let mut display = frostsnap_widgets::SuperDrawTarget::new(
            display,
            frostsnap_widgets::palette::PALETTE.background,
        );

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
                widget_with_debug.set_constraints(display.bounding_box().size);

                let mut last_touch: Option<Point> = None;
                let mut current_widget_index = 0usize;

                // Track last redraw time
                let mut last_redraw_time = timer.now();

                // Clear the screen with background color
                let _ = display.clear(PALETTE.background);

                // Main loop
                loop {
                    let now = timer.now();
                    let now_ms = frostsnap_widgets::Instant::from_millis(
                        now.duration_since_epoch().to_millis(),
                    );

                    // Process all pending touch events
                    touch_handler::process_all_touch_events(
                        &mut touch_receiver,
                        &mut widget_with_debug,
                        &mut last_touch,
                        &mut current_widget_index,
                        now_ms,
                    );

                    // Only redraw if enough time has passed since last redraw
                    let elapsed_ms = (now - last_redraw_time).to_millis();
                    if elapsed_ms >= DISPLAY_REFRESH_MS {
                        // Update last redraw time
                        last_redraw_time = now;
                        // Draw the UI stack (includes debug stats overlay)
                        let _ = widget_with_debug.draw(&mut display, now_ms);
                    }
                }
            }};
        }

        // Use the demo_widget! macro from frostsnap_widgets
        frostsnap_widgets::demo_widget!(DEMO, run_widget);
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
