//! Device resources including provisioned crypto state and partitions

use alloc::boxed::Box;
use core::cell::RefCell;
use esp_storage::FlashStorage;
use frostsnap_comms::{Downstream, Upstream};
use rand_chacha::ChaCha20Rng;

use crate::{
    ds::HardwareDs,
    efuse::EfuseHmacKeys,
    flash::VersionedFactoryData,
    frosty_ui::FrostyUi,
    io::SerialInterface,
    ota::OtaPartitions,
    partitions::{EspFlashPartition, Partitions},
    peripherals::DevicePeripherals,
};

/// Type alias for serial interfaces
type Serial<'a, D> = SerialInterface<'a, Timer<Timer0<TIMG0>, Blocking>, D>;
use esp_hal::{
    gpio::{AnyPin, Input},
    peripherals::TIMG0,
    rsa::Rsa,
    sha::Sha,
    timer::timg::{Timer, Timer0},
    uart::Uart,
    usb_serial_jtag::UsbSerialJtag,
    Blocking,
};

/// Device resources containing provisioned state and runtime partitions
pub struct Resources<'a> {
    /// Provisioned RNG with device entropy mixed in
    pub rng: ChaCha20Rng,

    /// HMAC keys from efuses
    pub hmac_keys: EfuseHmacKeys<'a>,

    /// Hardware Ds for attestation (None for dev devices)
    pub ds: Option<HardwareDs<'a>>,

    /// RSA hardware accelerator
    pub rsa: Rsa<'a, Blocking>,

    /// Factory certificate (for production devices)
    pub certificate: Option<frostsnap_comms::genuine_certificate::Certificate>,

    /// NVS partition for mutation log
    pub nvs: EspFlashPartition<'a>,

    /// OTA partitions for firmware updates
    pub ota: OtaPartitions<'a>,

    /// User interface
    pub ui: FrostyUi<'a>,

    // Runtime peripherals needed by esp32_run
    pub timer: &'a Timer<Timer0<TIMG0>, Blocking>,
    pub sha256: Sha<'a>,
    pub upstream_serial: Serial<'a, Upstream>,
    pub downstream_serial: Serial<'a, Downstream>,
    pub downstream_detect: Input<'a, AnyPin>,
}

impl<'a> Resources<'a> {
    /// Create serial interfaces from UARTs and JTAG
    fn create_serial_interfaces(
        timer: &'static Timer<Timer0<TIMG0>, Blocking>,
        uart_upstream: Option<Uart<'static, Blocking>>,
        uart_downstream: Uart<'static, Blocking>,
        jtag: UsbSerialJtag<'a, Blocking>,
        upstream_detect: &Input<'a, AnyPin>,
    ) -> (Serial<'a, Upstream>, Serial<'a, Downstream>) {
        let detect_device_upstream = upstream_detect.is_low();
        let upstream_serial = if detect_device_upstream {
            log!("upstream set to uart");
            let uart = uart_upstream.expect("upstream UART should exist when detected");
            SerialInterface::new_uart(uart, crate::uart_interrupt::UartNum::Uart1, timer)
        } else {
            log!("upstream set to jtag");
            SerialInterface::new_jtag(jtag, timer)
        };

        let downstream_serial = SerialInterface::new_uart(
            uart_downstream,
            crate::uart_interrupt::UartNum::Uart0,
            timer,
        );

        (upstream_serial, downstream_serial)
    }
    /// Initialize resources for production device
    /// Factory data is required for production devices
    pub fn init_production(
        peripherals: Box<DevicePeripherals<'a>>,
        flash: &'a RefCell<FlashStorage>,
    ) -> Box<Self> {
        let (partitions, factory_data) = Self::read_flash_data(flash);

        // Production devices must have factory data
        let factory_data = factory_data.expect("Production device must have factory data");

        // Production devices must be provisioned at the factory
        if !peripherals.efuse.has_hmac_keys_initialized() {
            panic!("Production device must be provisioned at the factory!");
        }

        // Destructure peripherals to take what we need
        let DevicePeripherals {
            timer,
            ui_timer,
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
            ..
        } = *peripherals;

        // Load existing keys using the moved hmac
        let mut hmac_keys = EfuseHmacKeys::load(&efuse, hmac.clone())
            .expect("Failed to load HMAC keys from efuses");
        let rng: ChaCha20Rng = hmac_keys.fixed_entropy.mix_in_rng(&mut initial_rng);

        // Create UI with display and touch receiver (using ui_timer)
        let ui = FrostyUi::new(display, touch_receiver, ui_timer);

        // Extract factory data
        let factory = factory_data.into_factory_data();

        // Create HardwareDs for production devices
        let ds = Some(HardwareDs::new(ds, factory.ds_encrypted_params.clone()));

        let rsa = Rsa::new(rsa);

        // Extract certificate from factory data
        let certificate = Some(factory.certificate);

        // Create serial interfaces
        let (upstream_serial, downstream_serial) = Self::create_serial_interfaces(
            timer,
            uart_upstream,
            uart_downstream,
            jtag,
            &upstream_detect,
        );

        Box::new(Self {
            rng,
            hmac_keys,
            ds,
            rsa,
            certificate,
            nvs: partitions.nvs,
            ota: partitions.ota,
            ui,
            timer,
            sha256,
            upstream_serial,
            downstream_serial,
            downstream_detect,
        })
    }

    /// Initialize resources for development device
    /// Factory data is optional for dev devices
    pub fn init_dev(
        peripherals: Box<DevicePeripherals<'a>>,
        flash: &'a RefCell<FlashStorage>,
    ) -> Box<Self> {
        let (partitions, factory_data) = Self::read_flash_data(flash);

        // Dev devices must be provisioned before reaching this point
        if !peripherals.efuse.has_hmac_keys_initialized() {
            panic!("Dev device must be provisioned before initialization!");
        }

        // Destructure peripherals to take what we need
        let DevicePeripherals {
            timer,
            ui_timer,
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
            ..
        } = *peripherals;

        // Load existing keys using the moved hmac
        let mut hmac_keys = EfuseHmacKeys::load(&efuse, hmac.clone())
            .expect("Failed to load HMAC keys from efuses");
        let rng: ChaCha20Rng = hmac_keys.fixed_entropy.mix_in_rng(&mut initial_rng);

        // Create UI with display and touch receiver (using ui_timer)
        let ui = FrostyUi::new(display, touch_receiver, ui_timer);

        // Create HardwareDs if factory data is present (dev devices might have it)
        let (ds, certificate) = if let Some(factory_data) = factory_data {
            let factory = factory_data.into_factory_data();
            (
                Some(HardwareDs::new(ds, factory.ds_encrypted_params)),
                Some(factory.certificate),
            )
        } else {
            // Dev device without factory data - no hardware RSA
            (None, None)
        };

        let rsa = Rsa::new(rsa);

        // Create serial interfaces
        let (upstream_serial, downstream_serial) = Self::create_serial_interfaces(
            timer,
            uart_upstream,
            uart_downstream,
            jtag,
            &upstream_detect,
        );

        Box::new(Self {
            rng,
            hmac_keys,
            ds,
            certificate,
            rsa,
            nvs: partitions.nvs,
            ota: partitions.ota,
            ui,
            timer,
            sha256,
            upstream_serial,
            downstream_serial,
            downstream_detect,
        })
    }

    /// Read flash partitions and data common to both dev and prod
    fn read_flash_data(
        flash: &'a RefCell<FlashStorage>,
    ) -> (Partitions<'a>, Option<VersionedFactoryData>) {
        // Load all partitions
        let partitions = Partitions::load(flash);

        // Try to read factory data (may not exist on dev devices)
        let factory_data = VersionedFactoryData::read(partitions.factory_data).ok();

        (partitions, factory_data)
    }
}
