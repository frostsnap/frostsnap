use crate::factory::screen_test;
use crate::flash::FactoryData;
use alloc::rc::Rc;
use core::cell::RefCell;
use cst816s::CST816S;
use embedded_graphics::{
    mono_font::{ascii::*, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
};
use embedded_hal as hal;
use embedded_text::{alignment::HorizontalAlignment, style::TextBoxStyleBuilder, TextBox};
use esp_hal::time::Duration;
use esp_hal::{hmac::Hmac, timer, usb_serial_jtag::UsbSerialJtag, Blocking};
use esp_storage::FlashStorage;
use frostsnap_comms::{factory::*, ReceiveSerial};
use frostsnap_embedded::ABWRITE_BINCODE_CONFIG;
use rand_core::{RngCore, SeedableRng};

use crate::{
    efuse::{self, EfuseHmacKeys},
    io::SerialInterface,
};

macro_rules! text_display {
    ($display:ident, $text:expr) => {
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

#[allow(clippy::too_many_arguments)]
pub fn run_factory<'a, S, I2C, PINT, RST, T>(
    display: &mut S,
    capsense: &mut CST816S<I2C, PINT, RST>,
    efuse: &efuse::EfuseController,
    hal_hmac: Rc<RefCell<Hmac<'a>>>,
    mut rng: impl rand_core::RngCore, // take ownership to stop caller from accidentally using it again
    jtag: &mut UsbSerialJtag<'a, Blocking>,
    timer: &'a T,
) -> (impl rand_core::RngCore, EfuseHmacKeys<'a>)
where
    I2C: hal::i2c::I2c,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin,
    S: DrawTarget<Color = Rgb565> + OriginDimensions,
    T: timer::Timer,
{
    let mut upstream = SerialInterface::<T, FactoryUpstream>::new_jtag(jtag, timer);
    let flash = RefCell::new(FlashStorage::new());
    let mut partitions = crate::partitions::Partitions::load(&flash);

    // if already configured we skip everything
    let efuses_burnt = EfuseHmacKeys::has_been_initialized();

    let mut hmac_keys = if efuses_burnt {
        EfuseHmacKeys::load(hal_hmac.clone()).expect("we should have hmac keys!")
    } else if cfg!(feature = "genuine_device") {
        screen_test::run(display, capsense);

        text_display!(
            display,
            "Device not configured, waiting for factory magic bytes!"
        );
        loop {
            if upstream.find_and_remove_magic_bytes() {
                upstream.write_magic_bytes().expect("can write magic bytes");
                text_display!(display, "Got factory magic bytes");
                break;
            }
        }

        let factory_entropy = read_message!(upstream, FactorySend::InitEntropy);

        text_display!(display, "Got entropy");
        upstream.send(DeviceFactorySend::InitEntropyOk).unwrap();

        let Esp32DsKey {
            encrypted_params,
            ds_hmac_key,
        } = read_message!(upstream, FactorySend::SetEsp32DsKey);
        // We don't immediately burn the RSA efuse, we do this after writing the blob

        upstream.send(DeviceFactorySend::ReceivedDsKey).unwrap();
        text_display!(display, "Received DS key");

        let certificate = read_message!(upstream, FactorySend::SetGenuineCertificate);
        let factory_data = FactoryData::init(encrypted_params.clone(), certificate.clone());

        let _ = partitions
            .factory_data
            .erase_and_write_this::<{ frostsnap_embedded::WRITE_BUF_SIZE }>(&factory_data)
            .unwrap();
        drop(factory_data);
        // double check it was written successfully
        let _rea_factory_data = bincode::decode_from_reader::<FactoryData, _, _>(
            partitions.factory_data.bincode_reader(),
            ABWRITE_BINCODE_CONFIG,
        )
        .expect("we should have been able to read the factory data back out!");

        let mut factory_rng = rand_chacha::ChaCha20Rng::from_seed(factory_entropy);
        let mut share_encryption_key = [0u8; 32];
        factory_rng.fill_bytes(&mut share_encryption_key);

        // Burn EFUSES, read protect since factory
        let read_protect = true;
        let _ = EfuseHmacKeys::init_with_keys(
            efuse,
            hal_hmac.clone(),
            read_protect,
            share_encryption_key,
            factory_entropy,
            ds_hmac_key,
        )
        .unwrap();

        text_display!(
            display,
            "Saved encrypted params, certificate and burnt efuse!"
        );

        esp_hal::reset::software_reset();
        unreachable!()
    } else {
        // Dev device - generate everything locally

        // Warn before irreversibly initializing a dev-device
        const COUNTDOWN_SECONDS: u32 = 30;
        for seconds_remaining in (1..=COUNTDOWN_SECONDS).rev() {
            text_display!(
                display,
                &format!(
                    "WARNING\n\nDev-Device Initialization\nAbout to burn eFuses!\n\n{} seconds remaining...\n\nUnplug now to cancel!",
                    seconds_remaining
                )
            );

            // Wait 1 second
            let start = timer.now();
            while timer.now().checked_duration_since(start).unwrap() < Duration::millis(1000) {}
        }

        text_display!(display, "Dev mode: generating keys locally");

        // Generate keys without external entropy our entropy
        let mut dev_entropy = [0u8; 32];
        rng.fill_bytes(&mut dev_entropy);
        let mut dev_rng = rand_chacha::ChaCha20Rng::from_seed(dev_entropy);

        let mut share_encryption_key = [0u8; 32];
        dev_rng.fill_bytes(&mut share_encryption_key);

        // MEANINGLESS DS HMAC KEY
        // Since the DS peripheral is only useful for signing with a key that's provisioned by the
        // factory to prove genuineness, and dev-boards are not genuine, this key serves no meaningful
        // security purpose on a non-genuine device.
        let mut ds_hmac_key = [0u8; 32];
        dev_rng.fill_bytes(&mut ds_hmac_key);

        // For dev devices, we skip creating factory data since we don't have a certificate, and
        // encrypted params are meaningless without genuine DS keys to use them with.
        // The device_keypair or otherwise will handle any signing necessary.

        // Burn efuses with dev keys (no read protection for dev devices)
        let read_protect = false;
        EfuseHmacKeys::init_with_keys(
            efuse,
            hal_hmac.clone(),
            read_protect,
            share_encryption_key,
            dev_entropy,
            ds_hmac_key,
        )
        .expect("Failed to initialize dev HMAC keys")
    };

    let final_rng = hmac_keys.fixed_entropy.mix_in_rng(&mut rng);
    (final_rng, hmac_keys)
}
