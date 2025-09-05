use crate::factory::screen_test;
use crate::flash::VersionedFactoryData;
use crate::peripherals::DevicePeripherals;
use alloc::boxed::Box;
use core::cell::RefCell;
use embedded_graphics::{
    mono_font::{ascii::*, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
};
use embedded_text::{alignment::HorizontalAlignment, style::TextBoxStyleBuilder, TextBox};
use esp_storage::FlashStorage;
use frostsnap_comms::{factory::*, ReceiveSerial};
use frostsnap_embedded::ABWRITE_BINCODE_CONFIG;
use rand_core::{RngCore, SeedableRng};

use crate::{
    efuse::EfuseKeyWriter,
    io::SerialInterface,
};

/// Configuration for device provisioning
pub struct ProvisioningConfig {
    /// Whether to read-protect the efuse keys
    pub read_protect: bool,
}

macro_rules! text_display {
    ($display:expr, $text:expr) => {
        let _ = $display.clear(Rgb565::BLACK);
        let _ = TextBox::with_textbox_style(
            $text,
            Rectangle::new(Point::new(0, 20), $display.size()),
            MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Center)
                .build(),
        )
        .draw($display);
    };
}

macro_rules! read_message {
    ($upstream:expr, FactorySend::$var:ident) => {
        loop {
            match $upstream.receive() {
                Some(Ok(ReceiveSerial::MagicBytes(_))) => { /* do nothing */ }
                Some(Ok(message)) => {
                    if let ReceiveSerial::Message(FactorySend::$var(inner)) = message {
                        break inner;
                    } else {
                        panic!("expecting {} got {:?}", stringify!($var), message);
                    }
                }
                Some(Err(e)) => {
                    panic!("error trying to read {}: {e}", stringify!($var));
                }
                None => { /* try again */ }
            }
        }
    };
}

/// Run dev device provisioning - burns efuses locally without factory communication
/// This function never returns - it resets the device after provisioning
pub fn run_dev_provisioning(peripherals: Box<DevicePeripherals<'_>>) -> ! {
    // Destructure what we need
    let DevicePeripherals {
        mut display,
        efuse,
        mut initial_rng,
        timer,
        ..
    } = *peripherals;

    use embedded_graphics::{
        mono_font::{ascii::FONT_10X20, MonoTextStyle},
        pixelcolor::Rgb565,
        prelude::*,
        primitives::Rectangle,
    };
    use embedded_text::{alignment::HorizontalAlignment, style::TextBoxStyleBuilder, TextBox};
    use esp_hal::{time::Duration, timer::Timer};

    // Warning countdown before burning efuses
    const COUNTDOWN_SECONDS: u32 = 30;
    for seconds_remaining in (1..=COUNTDOWN_SECONDS).rev() {
        let _ = display.clear(Rgb565::BLACK);
        let text = alloc::format!(
            "WARNING\n\nDev-Device Initialization\nAbout to burn eFuses!\n\n{} seconds remaining...\n\nUnplug now to cancel!",
            seconds_remaining
        );

        let _ = TextBox::with_textbox_style(
            &text,
            Rectangle::new(Point::new(0, 20), display.size()),
            MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
            TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Center)
                .build(),
        )
        .draw(&mut display);

        // Wait 1 second
        let start = timer.now();
        while timer.now().checked_duration_since(start).unwrap() < Duration::millis(1000) {}
    }

    // Show provisioning message
    let _ = display.clear(Rgb565::BLACK);
    let _ = TextBox::with_textbox_style(
        "Dev mode: generating keys locally",
        Rectangle::new(Point::new(0, 20), display.size()),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
        TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .build(),
    )
    .draw(&mut display);

    // Generate share encryption key
    let mut share_encryption_key = [0u8; 32];
    initial_rng.fill_bytes(&mut share_encryption_key);

    // Generate fixed entropy key
    let mut fixed_entropy_key = [0u8; 32];
    initial_rng.fill_bytes(&mut fixed_entropy_key);

    // Initialize efuses WITHOUT read protection (for dev devices)
    // No DS key needed for dev devices
    EfuseKeyWriter::new(&efuse)
        .read_protect(false)
        .add_encryption_key(share_encryption_key)
        .add_entropy_key(fixed_entropy_key)
        .write_efuses()
        .expect("Failed to initialize dev HMAC keys");

    // Show completion
    let _ = display.clear(Rgb565::BLACK);
    let _ = TextBox::with_textbox_style(
        "Dev device initialized!\n\nRestarting...",
        Rectangle::new(Point::new(0, 20), display.size()),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
        TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .build(),
    )
    .draw(&mut display);

    // Reset the device
    esp_hal::reset::software_reset();
    unreachable!()
}

/// Run factory provisioning for a device that needs provisioning
/// This function never returns - it resets the device after provisioning
pub fn run_factory_provisioning(peripherals: Box<DevicePeripherals<'_>>, config: ProvisioningConfig) -> ! {
    // Destructure what we need
    let DevicePeripherals {
        mut display,
        mut capsense,
        efuse,
        mut jtag,
        timer,
        ..
    } = *peripherals;

    // Initialize serial interface for factory communication
    let mut upstream = SerialInterface::<_, FactoryUpstream>::new_jtag(&mut jtag, &timer);

    // Initialize flash and partitions
    let flash = RefCell::new(FlashStorage::new());
    let mut partitions = crate::partitions::Partitions::load(&flash);

    // Run screen test
    screen_test::run(&mut display, &mut capsense);

    text_display!(
        &mut display,
        "Device not configured, waiting for factory magic bytes!"
    );

    // Wait for factory magic bytes
    loop {
        if upstream.find_and_remove_magic_bytes() {
            upstream.write_magic_bytes().expect("can write magic bytes");
            text_display!(&mut display, "Got factory magic bytes");
            break;
        }
    }

    // Receive factory entropy
    let factory_entropy = read_message!(upstream, FactorySend::InitEntropy);
    text_display!(&mut display, "Got entropy");
    upstream.send(DeviceFactorySend::InitEntropyOk).unwrap();

    // Receive DS key
    let Esp32DsKey {
        encrypted_params,
        ds_hmac_key,
    } = read_message!(upstream, FactorySend::SetEsp32DsKey);
    upstream.send(DeviceFactorySend::ReceivedDsKey).unwrap();
    text_display!(&mut display, "Received DS key");

    // Receive certificate
    let certificate = read_message!(upstream, FactorySend::SetGenuineCertificate);

    // Write factory data to flash
    let factory_data = VersionedFactoryData::init(encrypted_params.clone(), certificate.clone());
    partitions
        .factory_data
        .erase_and_write_this::<{ frostsnap_embedded::WRITE_BUF_SIZE }>(&factory_data)
        .unwrap();
    drop(factory_data);

    // Verify it was written successfully
    let _read_factory_data = bincode::decode_from_reader::<VersionedFactoryData, _, _>(
        partitions.factory_data.bincode_reader(),
        ABWRITE_BINCODE_CONFIG,
    )
    .expect("we should have been able to read the factory data back out!");

    // Generate share encryption key
    let mut factory_rng = rand_chacha::ChaCha20Rng::from_seed(factory_entropy);
    let mut share_encryption_key = [0u8; 32];
    factory_rng.fill_bytes(&mut share_encryption_key);

    // Burn EFUSES with configurable read protection
    EfuseKeyWriter::new(&efuse)
        .read_protect(config.read_protect)
        .add_encryption_key(share_encryption_key)
        .add_entropy_key(factory_entropy)
        .add_ds_key(ds_hmac_key)
        .write_efuses()
        .unwrap();

    text_display!(
        &mut display,
        "Saved encrypted params, certificate and burnt efuse!"
    );

    // Reset the device
    esp_hal::reset::software_reset();
    unreachable!()
}
