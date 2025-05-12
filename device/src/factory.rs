use crate::ds::ds_sign;
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
use frostsnap_comms::{factory::*, ReceiveSerial};
use rand_core::SeedableRng;

mod screen_test;

use crate::{
    efuse::{self, EfuseController, EfuseHmacKeys, KeyPurpose},
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

    let mut rng = extract_entropy(&mut rng, sha256, 1024, &factory_entropy[..]);
    text_display!(display, "Got entropy");
    upstream.send(DeviceFactorySend::InitEntropyOk).unwrap();

    let Esp32DsKey {
        encrypted_params,
        hmac_key,
    } = read_message!(upstream, FactorySend::SetEsp32DsKey);

    if !EfuseController::is_key_written(RSA_EFUSE_KEY_SLOT) {
        efuse
            .set_efuse_key(RSA_EFUSE_KEY_SLOT, KeyPurpose::Ds, false, hmac_key)
            .unwrap();
    }

    // esp32 test vector
    let challenge = frostsnap_core::hex::decode("354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b").unwrap();
    let signature = ds_sign(ds, encrypted_params, challenge);

    let debug_hex = signature
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<alloc::string::String>();
    text_display!(display, &format!("Sig: {}", debug_hex));

    // let value = efuse.read_efuse(RSA_EFUSE_KEY_SLOT).unwrap();
    // panic!(
    //     "efuse {}",
    //     frostsnap_core::schnorr_fun::fun::hex::encode(&value)
    // );

    loop {}

    // loop {
    //     let message: FactorySend = bincode::decode_from_reader(&mut upstream, BINCODE_CONFIG)
    //         .expect("failed to decode message");

    //     match message {
    //         FactorySend::SetEsp32DsKey(esp32_ds_key) => {
    //             efuse.randomly_init_key(3, KeyPurpose::Ds, true, rng)
    //         }
    //     }
    // }

    // extract more entropy from the trng that we theoretically need

    // if !EfuseHmacKeys::has_been_initialized() {
    //     screen_test::run(display, capsense);
    // }

    let do_read_protect = cfg!(feature = "read_protect_hmac_key");
    let mut hmac_keys =
        EfuseHmacKeys::<'a>::load_or_init(efuse, hal_hmac, do_read_protect, &mut rng)
            .expect("error during hmac efuse init");
    let final_rng = hmac_keys.fixed_entropy.mix_in_rng(&mut rng);

    (rng, hmac_keys)
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
