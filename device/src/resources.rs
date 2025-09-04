//! Device resources including provisioned crypto state and partitions

use alloc::boxed::Box;
use core::cell::RefCell;
use esp_storage::FlashStorage;
use rand_chacha::ChaCha20Rng;

use crate::{
    ds::HardwareRsa,
    efuse::EfuseHmacKeys,
    flash::FactoryData,
    ota::OtaPartitions,
    partitions::{EspFlashPartition, Partitions},
    peripherals::DevicePeripherals,
    ui::FrostyUi,
};
use esp_hal::{
    gpio::{AnyPin, Input},
    peripherals::TIMG0,
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

    /// Hardware RSA for attestation (None for dev devices)
    pub hardware_rsa: Option<HardwareRsa<'a>>,

    /// Factory certificate (for production devices)
    pub certificate: Option<frostsnap_comms::genuine_certificate::Certificate>,

    /// NVS partition for mutation log
    pub nvs: EspFlashPartition<'a>,

    /// OTA partitions for firmware updates
    pub ota: OtaPartitions<'a>,

    /// User interface
    pub ui: FrostyUi<'a>,

    // Runtime peripherals needed by esp32_run
    pub timer: Timer<Timer0<TIMG0>, Blocking>,
    pub sha256: Sha<'a>,
    pub uart_upstream: Option<Uart<'a, Blocking>>,
    pub uart_downstream: Uart<'a, Blocking>,
    pub jtag: UsbSerialJtag<'a, Blocking>,
    pub upstream_detect: Input<'a, AnyPin>,
    pub downstream_detect: Input<'a, AnyPin>,
}

impl<'a> Resources<'a> {
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
        if !EfuseHmacKeys::has_been_initialized() {
            panic!("Production device must be provisioned at the factory!");
        }

        // Destructure peripherals to take what we need
        let DevicePeripherals {
            timer,
            ui_timer,
            display,
            capsense,
            sha256,
            ds,
            hmac,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            mut initial_rng,
            ..
        } = *peripherals;

        // Load existing keys using the moved hmac
        let mut hmac_keys =
            EfuseHmacKeys::load(hmac.clone()).expect("Failed to load HMAC keys from efuses");
        let rng: ChaCha20Rng = hmac_keys.fixed_entropy.mix_in_rng(&mut initial_rng);

        // Create UI with display and capsense (using ui_timer)
        let ui = FrostyUi::new(display, capsense, ui_timer);

        // Create HardwareRsa for production devices
        let hardware_rsa = Some(HardwareRsa::new(
            ds,
            factory_data.encrypted_params().to_vec(),
        ));

        // Extract certificate from factory data
        let certificate = Some(factory_data.certificate().clone());

        Box::new(Self {
            rng,
            hmac_keys,
            hardware_rsa,
            certificate,
            nvs: partitions.nvs,
            ota: partitions.ota,
            ui,
            timer,
            sha256,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
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
        if !EfuseHmacKeys::has_been_initialized() {
            panic!("Dev device must be provisioned before initialization!");
        }

        // Destructure peripherals to take what we need
        let DevicePeripherals {
            timer,
            ui_timer,
            display,
            capsense,
            sha256,
            ds,
            hmac,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            mut initial_rng,
            ..
        } = *peripherals;

        // Load existing keys using the moved hmac
        let mut hmac_keys =
            EfuseHmacKeys::load(hmac.clone()).expect("Failed to load HMAC keys from efuses");
        let rng: ChaCha20Rng = hmac_keys.fixed_entropy.mix_in_rng(&mut initial_rng);


        // Create UI with display and capsense (using ui_timer)
        let ui = FrostyUi::new(display, capsense, ui_timer);

        // Create HardwareRsa if factory data is present (dev devices might have it)
        let (hardware_rsa, certificate) = if let Some(factory_data) = factory_data {
            (
                Some(HardwareRsa::new(
                    ds,
                    factory_data.encrypted_params().to_vec(),
                )),
                Some(factory_data.certificate().clone()),
            )
        } else {
            // Dev device without factory data - no hardware RSA
            (None, None)
        };

        Box::new(Self {
            rng,
            hmac_keys,
            hardware_rsa,
            certificate,
            nvs: partitions.nvs,
            ota: partitions.ota,
            ui,
            timer,
            sha256,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
        })
    }

    /// Read flash partitions and data common to both dev and prod
    fn read_flash_data(flash: &'a RefCell<FlashStorage>) -> (Partitions<'a>, Option<FactoryData>) {
        // Load all partitions
        let partitions = Partitions::load(flash);

        // Try to read factory data (may not exist on dev devices)
        let factory_data = FactoryData::read(partitions.factory_data).ok();

        (partitions, factory_data)
    }
}
