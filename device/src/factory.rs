use cst816s::CST816S;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_hal as hal;
use esp_hal::hmac::Hmac;
use rand_core::SeedableRng;

mod screen_test;

use crate::efuse::{self, EfuseHmacKeys};

pub fn run_factory<'a, S, I2C, PINT, RST>(
    display: &mut S,
    capsense: &mut CST816S<I2C, PINT, RST>,
    efuse: &efuse::EfuseController,
    hal_hmac: &'a core::cell::RefCell<Hmac<'a>>,
    mut rng: impl rand_core::RngCore, // take ownership to stop caller from accidentally using it again
    sha256: &mut esp_hal::sha::Sha<'_>,
) -> (impl rand_core::RngCore, EfuseHmacKeys<'a>)
where
    I2C: hal::i2c::I2c,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin,
    S: DrawTarget<Color = Rgb565> + OriginDimensions,
{
    // extract more entropy from the trng that we theoretically need
    let mut second_rng = extract_entropy(&mut rng, sha256, 1024);

    if !EfuseHmacKeys::has_been_initialized() {
        screen_test::run(display, capsense);
    }

    let do_read_protect = cfg!(feature = "read_protect_hmac_key");
    let mut hmac_keys =
        EfuseHmacKeys::<'a>::load_or_init(efuse, hal_hmac, do_read_protect, &mut second_rng)
            .expect("error during hmac efuse init");
    let final_rng = hmac_keys.fixed_entropy.mix_in_rng(&mut second_rng);

    (final_rng, hmac_keys)
}

pub fn extract_entropy(
    rng: &mut impl rand_core::RngCore,
    sha256: &mut esp_hal::sha::Sha<'_>,
    bytes: usize,
) -> impl rand_core::RngCore {
    pub use frostsnap_core::sha2::digest::FixedOutput;
    let mut digest = sha256.start::<esp_hal::sha::Sha256>();
    for _ in 0..(bytes.div_ceil(64)) {
        let mut entropy = [0u8; 64];
        rng.fill_bytes(&mut entropy);
        digest.update(&entropy).expect("infallible");
    }

    let result = digest.finalize_fixed();
    rand_chacha::ChaCha20Rng::from_seed(result.into())
}
