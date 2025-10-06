//! Production device binary

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;
use embedded_graphics::prelude::*;
use esp_hal::{entry, timer::Timer as _};
use esp_storage::FlashStorage;
use frostsnap_device::{
    esp32_run, peripherals::DevicePeripherals, resources::Resources,
    touch_calibration::adjust_touch_point,
};
use frostsnap_widgets::{
    palette::PALETTE, DynWidget, Instant as WidgetInstant, ScreenTest, SuperDrawTarget, Widget,
};

#[entry]
fn main() -> ! {
    // Initialize heap
    esp_alloc::heap_allocator!(256 * 1024);

    // Initialize hardware
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = esp_hal::clock::CpuClock::max();
        config
    });

    // Initialize flash storage (must stay alive for partition references)
    let flash = RefCell::new(FlashStorage::new());

    // Initialize all device peripherals with initial RNG
    let device = DevicePeripherals::init(peripherals);

    // Check if the device needs factory provisioning
    if device.needs_factory_provisioning() {
        // Run screen test widget first
        // Extract components we need from device
        let DevicePeripherals {
            display,
            mut touch_receiver,
            timer,
            ui_timer,
            backlight,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            sha256,
            hmac,
            ds,
            rsa,
            efuse,
            initial_rng,
        } = *device;

        let device = {
            // Create Rc<RefCell> to share display between SuperDrawTarget and our code
            let display_rc = Rc::new(RefCell::new(display));
            let mut super_display =
                SuperDrawTarget::from_shared(Rc::clone(&display_rc), PALETTE.background);

            // Create and run screen test widget
            let mut screen_test_widget = ScreenTest::new();
            screen_test_widget.set_constraints(Size::new(240, 280));

            let mut last_redraw_time = timer.now();

            // Clear screen
            let _ = super_display.clear(PALETTE.background);

            // Run until widget is completed
            loop {
                let now = timer.now();
                let now_ms = WidgetInstant::from_millis(now.duration_since_epoch().to_millis());

                // Process touch events directly (without OverlayDebug)
                while let Some(touch_event) = touch_receiver.dequeue() {
                    let touch_point = adjust_touch_point(Point::new(touch_event.x, touch_event.y));
                    let is_release = touch_event.action == 1; // ACTION_LIFT_UP
                    screen_test_widget.handle_touch(touch_point, now_ms, is_release);
                }

                // Redraw if needed
                let elapsed_ms = (now - last_redraw_time).to_millis();
                if elapsed_ms >= 5 {
                    let _ = screen_test_widget.draw(&mut super_display, now_ms);
                    last_redraw_time = now;
                }

                // Exit when test is completed (HoldToConfirm finished)
                if screen_test_widget.is_completed() {
                    break;
                }
            }

            // Extract display back from Rc
            drop(super_display);
            let display = Rc::try_unwrap(display_rc)
                .unwrap_or_else(|_| panic!("should be only holder"))
                .into_inner();

            // Reconstruct DevicePeripherals
            Box::new(DevicePeripherals {
                display,
                touch_receiver,
                timer,
                ui_timer,
                backlight,
                uart_upstream,
                uart_downstream,
                jtag,
                upstream_detect,
                downstream_detect,
                sha256,
                hmac,
                ds,
                rsa,
                efuse,
                initial_rng,
            })
        };

        // Now run factory provisioning
        let config = frostsnap_device::factory::init::ProvisioningConfig {
            read_protect: true, // Production devices should have read protection
        };
        frostsnap_device::factory::run_factory_provisioning(device, config);
    } else {
        // Device is already provisioned - proceed with normal boot
        let mut resources = Resources::init_production(device, &flash);

        // Run main event loop
        esp32_run::run(&mut resources);
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
