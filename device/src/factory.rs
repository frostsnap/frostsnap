use crate::ds::{ds_words_to_bytes, sign_like_test_vectors, standard_rsa_sign};
use crate::factory::DEVICE_SHOW_TEST_VECTOR;
use crate::flash::FactoryData;
use alloc::vec::Vec;
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
use esp_hal::{hmac::Hmac, peripherals::DS, timer, usb_serial_jtag::UsbSerialJtag, Blocking};
use esp_storage::FlashStorage;
use frostsnap_comms::{factory::*, ReceiveSerial};
use frostsnap_embedded::ABWRITE_BINCODE_CONFIG;
use rand_core::SeedableRng;

// mod screen_test;

use crate::{
    efuse::{self, EfuseHmacKeys, KeyPurpose},
    io::SerialInterface,
};

const RSA_EFUSE_KEY_SLOT: u8 = 4;

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

pub fn run_factory<'a, 'b, S, I2C, PINT, RST, T>(
    display: &mut S,
    capsense: &mut CST816S<I2C, PINT, RST>,
    efuse: &efuse::EfuseController,
    hal_hmac: &'a core::cell::RefCell<Hmac<'a>>,
    mut rng: impl rand_core::RngCore, // take ownership to stop caller from accidentally using it again
    sha256: &mut esp_hal::sha::Sha<'_>,
    jtag: &'b mut UsbSerialJtag<'a, Blocking>,
    timer: &'a T,
    ds: DS,
) -> (impl rand_core::RngCore, EfuseHmacKeys<'a>)
where
    I2C: hal::i2c::I2c,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin,
    S: DrawTarget<Color = Rgb565> + OriginDimensions,
    T: timer::Timer,
{
    let mut upstream = SerialInterface::<T, FactoryUpstream>::new_jtag(jtag, timer);

    text_display!(display, "waiting for factory magic bytes");

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
        hmac_key,
        challenge,
    } = read_message!(upstream, FactorySend::SetEsp32DsKey);

    if DEVICE_SHOW_TEST_VECTOR {
        // don't panic if already burned
        let _ = efuse.set_efuse_key(RSA_EFUSE_KEY_SLOT, KeyPurpose::Ds, false, hmac_key);
        let signature = sign_like_test_vectors(ds, encrypted_params, challenge);
        let dbg_signature = signature
            .iter()
            .map(|word| format!("{:08x}", word))
            .collect::<Vec<_>>()
            .join(" ");
        text_display!(display, &format!("Sig:\n{}", dbg_signature));
        loop {}
    }

    let _ = efuse.set_efuse_key(RSA_EFUSE_KEY_SLOT, KeyPurpose::Ds, true, hmac_key);
    let signature = standard_rsa_sign(ds, encrypted_params.clone(), &challenge);

    let signature = ds_words_to_bytes(&signature);
    upstream
        .send(DeviceFactorySend::SetDs { signature })
        .unwrap();
    text_display!(display, "Set DS and signed");

    let certificate = read_message!(upstream, FactorySend::SetGenuineCertificate);

    let factory_data = FactoryData::new(encrypted_params, certificate);

    let flash = RefCell::new(FlashStorage::new());
    let mut partitions = crate::partitions::Partitions::load(&flash);
    let _ = partitions
        .factory_data
        .erase_and_write_this::<{ frostsnap_embedded::WRITE_BUF_SIZE }>(factory_data)
        .unwrap();

    {
        // double check it was written successfully
        let _read_factory_data = bincode::decode_from_reader::<FactoryData, _, _>(
            partitions.factory_data.bincode_reader(),
            ABWRITE_BINCODE_CONFIG,
        )
        .expect("we should have been able to read the factory data back out!");
    }

    upstream
        .send(DeviceFactorySend::SavedGenuineCertificate)
        .unwrap();

    text_display!(display, "Saved encrypted params and certificate!");

    // Burn EFUSES

    loop {}

    let do_read_protect = cfg!(feature = "read_protect_hmac_key");
    let mut hmac_keys =
        EfuseHmacKeys::<'a>::load_or_init(efuse, hal_hmac, do_read_protect, &mut rng)
            .expect("error during hmac efuse init");
    let final_rng = hmac_keys.fixed_entropy.mix_in_rng(&mut rng);

    (final_rng, hmac_keys)
}

pub fn extract_entropy(
    rng: &mut impl rand_core::RngCore,
    sha256: &mut esp_hal::sha::Sha<'_>,
    bytes: usize,
    mix_in: &[u8],
) -> impl rand_core::RngCore {
    pub use frostsnap_core::sha2::digest::FixedOutput;
    let mut digest = sha256.start::<esp_hal::sha::Sha256>();
    for _ in 0..(bytes.div_ceil(64)) {
        let mut entropy = [0u8; 64];
        rng.fill_bytes(&mut entropy);
        digest.update(&entropy).expect("infallible");
    }
    digest.update(&mix_in).expect("infallible");

    let result = digest.finalize_fixed();
    rand_chacha::ChaCha20Rng::from_seed(result.into())
}
