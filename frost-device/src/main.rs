// use log::*;
use anyhow::{bail, Result};
use std::collections::BTreeMap;

use esp_idf_hal::gpio::*;
use esp_idf_hal::peripherals::Peripherals;

use std::thread::sleep;
use std::time::Duration;

use embedded_svc::http::client::Method::{Get, Post};

use schnorr_fun::Signature;
use schnorr_fun::{
    frost::{Frost, Nonce, PointPoly, ScalarPoly},
    fun::{marker::Public, Scalar},
    nonce::Deterministic,
    Message, Schnorr,
};
use sha2::Sha256;

pub mod http;
pub mod wifi;
pub mod ws2812;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
    #[default("")]
    frost_server: &'static str,
    #[default("2")]
    threshold: &'static str,
    #[default("2")]
    n_parties: &'static str,
}

fn post(url: impl AsRef<str>, data: String) -> Result<String> {
    http::request(Post, url, Some(&data))
}

fn get(url: impl AsRef<str>) -> Result<String> {
    http::request(Get, url, None)
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let mut button = PinDriver::input(peripherals.pins.gpio9)?;

    button.set_pull(Pull::Down)?;

    // Onboard RGB LED pin
    // ESP32-C3-DevKitC-02 gpio8, esp-rs gpio2
    let led = peripherals.pins.gpio2;
    let channel = peripherals.rmt.channel0;
    let mut neopixel = ws2812::NeoPixel::new(channel, led)?;
    neopixel.clear()?;

    // let mut i = 0;
    // loop {
    //     prompt_wait(format!("{}", i).as_str());
    //     neopixel.rainbow(3, 5, 30)?;
    //     i += 1;
    // }

    // WIFI stuff
    // Connect to the Wi-Fi network
    let app_config = CONFIG;
    let _wifi = match wifi::wifi(app_config.wifi_ssid, app_config.wifi_psk, peripherals.modem) {
        Ok(inner) => inner,
        Err(err) => {
            neopixel.error()?;
            bail!("could not connect to Wi-Fi network: {:?}", err)
        }
    };

    // Clear server state
    let url = CONFIG.frost_server.to_owned() + "/clear";
    match get(url) {
        Err(err) => {
            neopixel.error()?;
            bail!(
                "could not connect to FROST coordinating server: {} {:?}",
                CONFIG.frost_server,
                err
            )
        }
        Ok(_) => {
            neopixel.success()?;
        }
    };

    let mut prompt_wait = |s: &str| {
        println!(" ");
        println!("Press button to {}:", s);
        // button debounce
        // sleep(Duration::from_millis(200));
        neopixel.blink().unwrap();
        loop {
            // prevent wdt trigger
            sleep(Duration::from_millis(10));
            if button.is_low() {
                break;
            }
        }
    };

    // FROST
    //
    // We're going to carry out 2 parties on 1 device and later separate
    let threshold = CONFIG.threshold.parse::<usize>().unwrap();
    let n_parties = CONFIG.n_parties.parse::<usize>().unwrap();

    let frost = Frost::new(Schnorr::<Sha256, Deterministic<Sha256>>::new(
        Deterministic::<Sha256>::default(),
    ));
    dbg!(threshold, n_parties);
    assert!(threshold <= n_parties);

    // Create scalar polynomials
    prompt_wait(&"generate scalar polynomial and share point polynomial");
    let mut rng = rand::rngs::OsRng;
    let sp = ScalarPoly::random(threshold, &mut rng);
    println!("converting to point polys");
    let pp = sp.to_point_poly();

    // let sp2 = ScalarPoly::random(threshold, &mut rng);
    // let pp2 = sp2.to_point_poly();

    // Share point polynomials
    let url = CONFIG.frost_server.to_owned() + "/keygen";
    let response = post(
        url.clone(),
        serde_json::to_string(&(threshold, n_parties, pp)).unwrap(),
    )?;
    let id: usize = serde_json::from_str(&response)?;
    println!("Participant index: {}", id);
    println!("Sent point poly to coordinator!");

    // let response2 = post(url.clone(), serde_json::to_string(&pp2).unwrap().as_bytes())?;
    // let id2: usize = serde_json::from_str(&response2)?;

    // Receive point polynomials
    prompt_wait(&"fetch others the other parties' polynomials, create and send secret shares");
    let url = CONFIG.frost_server.to_owned() + "/receive_polys";
    let response = &get(url).expect("got point polys");
    let point_polys: BTreeMap<usize, PointPoly> = serde_json::from_str(&response)?;
    dbg!(&point_polys);
    let keygen = frost
        .new_keygen(point_polys.iter().map(|(_, p)| p.clone()).collect())
        .unwrap();
    // led.set_pixel(RGB8::new(25, 25, 50))?;

    // Send shares
    let (shares, proof_of_possesion) = frost.create_shares(&keygen, sp);
    dbg!(&shares);
    let url = CONFIG.frost_server.to_owned() + "/send_shares";
    post(
        url.clone(),
        serde_json::to_string(&(id, shares, proof_of_possesion)).unwrap(),
    )?;

    // let (shares2, proof_of_possesion2) = frost.create_shares(&keygen, sp2);
    // post(
    //     url.clone(),
    //     serde_json::to_string(&(id2, shares2, proof_of_possesion2))
    //         .unwrap()
    //         .as_bytes(),
    // )?;

    // Receive shares
    prompt_wait(&"receive secret shares and create frost key");
    let url = CONFIG.frost_server.to_owned() + "/receive_shares?i=" + &id.to_string();
    let response = &get(url).expect("got my shares");
    let (my_shares, my_pops): (Vec<Scalar>, Vec<Signature<Public>>) =
        serde_json::from_str(&response)?;
    let my_shares_zero = my_shares.into_iter().map(|s| s.mark_zero()).collect();
    let (secret_share, frost_key) =
        frost.finish_keygen_to_xonly(keygen.clone(), id, my_shares_zero, my_pops)?;

    // let url = CONFIG.frost_server.to_owned() + "/receive_shares?i=1";
    // let response = &get(url).expect("got my shares");
    // let (my_shares, my_pops): (Vec<Scalar>, Vec<Signature<Public>>) =
    //     serde_json::from_str(&response)?;
    // let my_shares_zero = my_shares.into_iter().map(|s| s.mark_zero()).collect();
    // let (secret_share2, frost_key2) =
    //     frost.finish_keygen_to_xonly(keygen, 1, my_shares_zero, my_pops)?;

    dbg!(&frost_key);
    println!("Created frost key!");
    // led.set_pixel(RGB8::new(10, 10, 50))?;

    // Signing
    println!("\n| SIGNING");
    println!("Message: {}", "test");
    let msg = Message::plain("test", b"test");
    prompt_wait(&"create nonces for signing and share");
    let verification_shares_bytes: Vec<_> = frost_key
        .verification_shares()
        .map(|share| share.to_bytes())
        .collect();
    let sid = [
        frost_key.public_key().to_xonly_bytes().as_slice(),
        verification_shares_bytes.concat().as_slice(),
        b"frost-device-test".as_slice(),
    ]
    .concat();
    let nonce = frost.gen_nonce(
        &secret_share,
        &[sid.as_slice(), &[id as u8]].concat(),
        Some(frost_key.public_key().normalize()),
        None,
    );
    dbg!(&nonce);

    // let nonce2 = frost.gen_nonce(
    //     &secret_share2,
    //     &[sid.as_slice(), &[1]].concat(),
    //     Some(frost_key.public_key().normalize()),
    //     None,
    // );

    // Send nonces
    let url = CONFIG.frost_server.to_owned() + "/send_nonce";
    let pub_nonce = nonce.public().clone();
    post(
        url.clone(),
        serde_json::to_string(&(id, pub_nonce)).unwrap(),
    )?;

    // let pub_nonce2 = nonce2.public().clone();
    // post(
    //     url.clone(),
    //     serde_json::to_string(&(id2, pub_nonce2))
    //         .unwrap()
    //         .as_bytes(),
    // )?;

    // Receive nonces
    prompt_wait(&"recieve nonces from other parties, sign the message and share partial signature");
    let url = CONFIG.frost_server.to_owned() + "/receive_nonces";
    let response = &get(url).expect("got nonces");
    let nonces: Vec<(usize, Nonce)> = serde_json::from_str(&response)?;
    dbg!(&nonces);
    println!("Received nonces..");

    // Sign
    let session = frost.start_sign_session(&frost_key, nonces.clone(), msg);
    let sig = frost.sign(&frost_key, &session, id, &secret_share, nonce);
    dbg!(&sig);
    println!("Signed, sharing partial sigs!");

    // let session2 = frost.start_sign_session(&frost_key2, nonces.clone(), msg);
    // dbg!(frost.verify_signature_share(&frost_key, &session, 0, sig));
    // let sig2 = frost.sign(&frost_key2, &session2, 1, &secret_share2, nonce2);

    // Send Sigs
    let url = CONFIG.frost_server.to_owned() + "/send_sig";
    post(url.clone(), serde_json::to_string(&(id, sig)).unwrap())?;

    // Receive signature shares
    prompt_wait(&"fetch signature shares from other parties, combine into final signature");
    let url = CONFIG.frost_server.to_owned() + "/receive_sigs";
    let response = &get(url).expect("get sigs");
    let sigs: Vec<(usize, Scalar)> = serde_json::from_str(&response)?;
    println!("Received signature shares..");

    // Validate signature shares
    for (i, sig) in sigs.clone() {
        dbg!(frost.verify_signature_share(&frost_key, &session, i, sig.mark_zero().public()));
    }

    let combined_sig = frost.combine_signature_shares(
        &frost_key,
        &session,
        sigs.iter()
            .map(|(_, sig)| sig.clone().mark_zero().public())
            .collect(),
    );
    dbg!(&combined_sig);
    println!("verifying final signature");
    if frost
        .schnorr
        .verify(&frost_key.public_key(), msg, &combined_sig)
    {
        println!("Valid signature!");
        neopixel.rainbow(0, 10, 10)?;
        // rainbow loop at 20% brightness
        // let mut i: u32 = 0;
        // loop {
        //     let rgb = ws2812::hsv2rgb(i, 100, 10)?;
        //     neopixel(rgb, &mut rmt)?;
        //     if i == 360 {
        //         i = 0;
        //     }
        //     i += 1;
        //     sleep(Duration::from_millis(10));
        // }
    } else {
        println!("Invalid signature :(");
        neopixel.error()?;
    }
    Ok(())
}
