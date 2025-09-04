//! Device resources including provisioned crypto state and partitions

use alloc::boxed::Box;
use core::cell::RefCell;
use esp_storage::FlashStorage;
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

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
        mut peripherals: Box<DevicePeripherals<'a>>,
        flash: &'a RefCell<FlashStorage>,
    ) -> Box<Self> {
        let (partitions, factory_data) = Self::read_flash_data(flash);

        // Move hmac out first since we'll need it either way
        let hmac = peripherals.hmac.clone();

        // Check if device needs provisioning and handle inline
        let (rng, hmac_keys) = if !EfuseHmacKeys::has_been_initialized() {
            // Dev provisioning with warning - inline to avoid lifetime issues
            use embedded_graphics::{
                mono_font::{ascii::FONT_10X20, MonoTextStyle},
                pixelcolor::Rgb565,
                prelude::*,
                primitives::Rectangle,
            };
            use embedded_text::{
                alignment::HorizontalAlignment, style::TextBoxStyleBuilder, TextBox,
            };
            use esp_hal::{time::Duration, timer::Timer};

            // Warning countdown before burning efuses
            const COUNTDOWN_SECONDS: u32 = 30;
            for seconds_remaining in (1..=COUNTDOWN_SECONDS).rev() {
                let _ = peripherals.display.clear(Rgb565::BLACK);
                let text = alloc::format!(
                    "WARNING\n\nDev-Device Initialization\nAbout to burn eFuses!\n\n{} seconds remaining...\n\nUnplug now to cancel!",
                    seconds_remaining
                );

                let _ = TextBox::with_textbox_style(
                    &text,
                    Rectangle::new(Point::new(0, 20), peripherals.display.size()),
                    MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
                    TextBoxStyleBuilder::new()
                        .alignment(HorizontalAlignment::Center)
                        .build(),
                )
                .draw(&mut peripherals.display);

                // Wait 1 second
                let start = peripherals.timer.now();
                while peripherals
                    .timer
                    .now()
                    .checked_duration_since(start)
                    .unwrap()
                    < Duration::millis(1000)
                {}
            }

            // Show provisioning message
            let _ = peripherals.display.clear(Rgb565::BLACK);
            let _ = TextBox::with_textbox_style(
                "Dev mode: generating keys locally",
                Rectangle::new(Point::new(0, 20), peripherals.display.size()),
                MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
                TextBoxStyleBuilder::new()
                    .alignment(HorizontalAlignment::Center)
                    .build(),
            )
            .draw(&mut peripherals.display);

            // Generate entropy for dev device
            let mut dev_entropy = [0u8; 32];
            peripherals.initial_rng.fill_bytes(&mut dev_entropy);
            let mut dev_rng = ChaCha20Rng::from_seed(dev_entropy);

            // Generate share encryption key
            let mut share_encryption_key = [0u8; 32];
            dev_rng.fill_bytes(&mut share_encryption_key);

            // Generate DS HMAC key (not used for attestation on dev devices)
            let mut ds_hmac_key = [0u8; 32];
            dev_rng.fill_bytes(&mut ds_hmac_key);

            // Initialize efuses WITHOUT read protection (for dev devices)
            let read_protect = false;

            // Generate fixed entropy key
            let mut fixed_entropy_key = [0u8; 32];
            dev_rng.fill_bytes(&mut fixed_entropy_key);

            let mut hmac_keys = EfuseHmacKeys::init_with_keys(
                &peripherals.efuse,
                hmac.clone(),
                read_protect,
                share_encryption_key,
                fixed_entropy_key,
                ds_hmac_key,
            )
            .expect("Failed to initialize HMAC keys");

            // Mix in device entropy to create final RNG
            let final_rng: ChaCha20Rng = hmac_keys
                .fixed_entropy
                .mix_in_rng(&mut peripherals.initial_rng);

            (final_rng, hmac_keys)
        } else {
            // Device already provisioned, load existing keys
            let mut hmac_keys =
                EfuseHmacKeys::load(hmac.clone()).expect("Failed to load HMAC keys from efuses");
            let rng: ChaCha20Rng = hmac_keys
                .fixed_entropy
                .mix_in_rng(&mut peripherals.initial_rng);
            (rng, hmac_keys)
        };

        // Destructure peripherals to take what we need
        let DevicePeripherals {
            timer,
            ui_timer,
            display,
            capsense,
            sha256,
            ds,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            ..
        } = *peripherals;

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
