//! Device resources including provisioned crypto state and partitions

use alloc::boxed::Box;
use core::cell::RefCell;
use esp_storage::FlashStorage;
use frostsnap_comms::{Downstream, Upstream};
use rand_chacha::ChaCha20Rng;

use crate::{
    crypto::{EspCrypto, HmacKey, HmacKeyIds},
    flash::FactoryDataHandle,
    frosty_ui::FrostyUi,
    io::SerialInterface,
    ota::OtaPartitions,
    partitions::{EspFlashPartition, Partitions},
    peripherals::DevicePeripherals,
};

/// Type alias for serial interfaces
type Serial<'a, D> = SerialInterface<'a, D>;
use esp_hal::{
    gpio::Input, hmac::KeyId, uart::Uart, usb::usb_serial_jtag::UsbSerialJtag, Blocking,
};
use rand_core::{RngCore, SeedableRng};

/// Device resources containing provisioned state and runtime partitions
pub struct Resources<'a> {
    /// Provisioned RNG with device entropy mixed in
    pub rng: ChaCha20Rng,

    pub hmac_keys: HmacKeyIds<KeyId>,

    /// NVS partition for mutation log
    pub nvs: EspFlashPartition<'a>,

    /// OTA partitions for firmware updates
    pub ota: OtaPartitions<'a>,

    /// User interface. Boxed because it's ~2.3KB — by far the largest field —
    /// and keeping it inline makes `Box::new(Self { .. })` in the init fns
    /// materialize the whole struct on the stack, blowing their stack frames
    /// past the CI stack-check limit.
    pub ui: Box<FrostyUi<'a>>,

    // Runtime peripherals needed by esp32_run
    pub upstream_serial: Serial<'a, Upstream>,
    pub downstream_serial: Serial<'a, Downstream>,
    pub downstream_detect: Input<'a>,
}

impl<'a> Resources<'a> {
    /// Create serial interfaces from UARTs and JTAG
    fn create_serial_interfaces(
        uart_upstream: Option<Uart<'static, Blocking>>,
        uart_downstream: Uart<'static, Blocking>,
        jtag: UsbSerialJtag<'a, Blocking>,
        upstream_detect: &Input<'a>,
    ) -> (Serial<'a, Upstream>, Serial<'a, Downstream>) {
        let detect_device_upstream = upstream_detect.is_low();
        let upstream_serial = if detect_device_upstream {
            log!("upstream set to uart");
            let uart = uart_upstream.expect("upstream UART should exist when detected");
            SerialInterface::new_uart(uart, crate::uart_interrupt::UartNum::Uart1)
        } else {
            log!("upstream set to jtag");
            SerialInterface::new_jtag(jtag)
        };

        let downstream_serial =
            SerialInterface::new_uart(uart_downstream, crate::uart_interrupt::UartNum::Uart0);

        (upstream_serial, downstream_serial)
    }
    /// Initialize resources for production device
    /// Factory data is required for production devices
    pub fn init_production(peripherals: Box<DevicePeripherals<'a>>) -> (Box<Self>, EspCrypto<'a>) {
        // Production devices must be provisioned at the factory
        if !peripherals.efuse.has_hmac_keys_initialized() {
            panic!("Production device must be provisioned at the factory!");
        }

        // Destructure peripherals to take what we need
        let DevicePeripherals {
            display,
            touch_receiver,
            sha256,
            ds,
            rsa,
            hmac,
            efuse,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            mut initial_rng,
            flash,
            ..
        } = *peripherals;

        let (partitions, factory_data) = Self::read_flash_data(flash);
        let factory_data = factory_data.expect("Production device must have factory data");

        let hmac_keys = efuse
            .hmac_key_ids()
            .expect("Failed to load HMAC keys from efuses");
        let mut crypto = EspCrypto::new(sha256, hmac, rsa, ds, Some(factory_data));
        let rng = mix_in_rng(
            &mut HmacKey::new(&mut crypto, hmac_keys.fixed_entropy),
            &mut initial_rng,
        );

        let ui = Box::new(FrostyUi::new(display, touch_receiver));

        // Create serial interfaces
        let (upstream_serial, downstream_serial) =
            Self::create_serial_interfaces(uart_upstream, uart_downstream, jtag, &upstream_detect);

        let resources = Box::new(Self {
            rng,
            hmac_keys,
            nvs: partitions.nvs,
            ota: partitions.ota,
            ui,
            upstream_serial,
            downstream_serial,
            downstream_detect,
        });
        (resources, crypto)
    }

    /// Initialize resources for development device
    /// Factory data is optional for dev devices
    pub fn init_dev(peripherals: Box<DevicePeripherals<'a>>) -> (Box<Self>, EspCrypto<'a>) {
        // Dev devices must be provisioned before reaching this point
        if !peripherals.efuse.has_hmac_keys_initialized() {
            panic!("Dev device must be provisioned before initialization!");
        }

        // Destructure peripherals to take what we need
        let DevicePeripherals {
            display,
            touch_receiver,
            sha256,
            ds,
            rsa,
            hmac,
            efuse,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            mut initial_rng,
            flash,
            ..
        } = *peripherals;

        let (partitions, factory_data) = Self::read_flash_data(flash);

        let hmac_keys = efuse
            .hmac_key_ids()
            .expect("Failed to load HMAC keys from efuses");
        let mut crypto = EspCrypto::new(sha256, hmac, rsa, ds, factory_data);
        let rng = mix_in_rng(
            &mut HmacKey::new(&mut crypto, hmac_keys.fixed_entropy),
            &mut initial_rng,
        );

        let ui = Box::new(FrostyUi::new(display, touch_receiver));

        // Create serial interfaces
        let (upstream_serial, downstream_serial) =
            Self::create_serial_interfaces(uart_upstream, uart_downstream, jtag, &upstream_detect);

        let resources = Box::new(Self {
            rng,
            hmac_keys,
            nvs: partitions.nvs,
            ota: partitions.ota,
            ui,
            upstream_serial,
            downstream_serial,
            downstream_detect,
        });
        (resources, crypto)
    }

    /// Read flash partitions and data common to both dev and prod
    fn read_flash_data(
        flash: FlashStorage<'static>,
    ) -> (Partitions<'a>, Option<FactoryDataHandle<'a>>) {
        // Resources is leaked, so the partitions' flash lives as long as it does.
        let partitions = Partitions::load(Box::leak(Box::new(RefCell::new(flash))));

        // Dev devices may have no factory data
        let factory_data = FactoryDataHandle::open(partitions.factory_cert);

        (partitions, factory_data)
    }
}

fn mix_in_rng(fixed_entropy: &mut HmacKey<'_, '_>, rng: &mut impl RngCore) -> ChaCha20Rng {
    let mut entropy = [0u8; 64];
    rng.fill_bytes(&mut entropy);
    ChaCha20Rng::from_seed(fixed_entropy.hash("mix-in-rng", &entropy))
}
