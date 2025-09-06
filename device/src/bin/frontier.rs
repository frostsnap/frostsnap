//! Production device binary

#![no_std]
#![no_main]

extern crate alloc;

use core::cell::RefCell;
use esp_hal::entry;
use esp_storage::FlashStorage;
use frostsnap_device::{esp32_run, peripherals::DevicePeripherals, resources::Resources};

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
        // Run factory provisioning - this will reset the device
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
