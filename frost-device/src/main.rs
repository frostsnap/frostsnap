use std::collections::{BTreeMap, BTreeSet, HashSet};

use esp32_c3_dkc02_bsc as bsc;
use esp_idf_sys as _;
use log::*;

use embedded_svc::io::Write;
use embedded_svc::{
    http::{
        client::{Client, Request, RequestWrite, Response},
        Status,
    },
    io::Read,
};
use esp_idf_svc::http::client::EspHttpClient;

use schnorr_fun::Signature;
use schnorr_fun::{
    frost::{Frost, Nonce, PointPoly, ScalarPoly},
    fun::{marker::Public, Scalar},
    nonce::Deterministic,
    Message, Schnorr,
};
use sha2::Sha256;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
    #[default("")]
    frost_server: &'static str,
}

fn get(url: impl AsRef<str>) -> anyhow::Result<String> {
    let mut client = EspHttpClient::new_default()?;
    let request = client.get(url.as_ref())?;

    let writer = request.into_writer(0)?;

    let mut response = writer.submit()?;
    let status = response.status();
    let mut total_size = 0;

    println!("response code: {}\n", status);

    let mut response_text = "".to_string();
    match status {
        200..=299 => {
            let mut buf = [0_u8; 256];
            let mut reader = response.reader();
            let total_size = loop {
                if let Ok(size) = Read::read(&mut reader, &mut buf) {
                    if size == 0 {
                        break total_size;
                    }
                    total_size += size;
                    response_text += &std::str::from_utf8(&buf[..size])?.to_string();
                    // println!("{}", response_text);
                }
            };
        }
        _ => anyhow::bail!("unexpected response code: {}", status),
    };
    Ok(response_text)
}

fn post(url: impl AsRef<str>, data: &[u8]) -> anyhow::Result<String> {
    let mut client = EspHttpClient::new_default()?;
    let request = client.post(url.as_ref())?;

    let mut writer = request.into_writer(data.len())?;
    writer.write(data)?;

    let mut response = writer.submit()?;
    let status = response.status();
    let mut total_size = 0;

    println!("response code: {}\n", status);

    let mut response_text = "".to_string();
    match status {
        200..=299 => {
            // 5. if the status is OK, read response data chunk by chunk into a buffer and print it until done
            let mut buf = [0_u8; 256];
            let mut reader = response.reader();
            loop {
                if let Ok(size) = Read::read(&mut reader, &mut buf) {
                    if size == 0 {
                        break;
                    }
                    total_size += size;
                    // 6. try converting the bytes into a Rust (UTF-8) string and print it
                    response_text += &std::str::from_utf8(&buf[..size])?.to_string();
                }
            }
        }
        _ => anyhow::bail!("unexpected response code: {}", status),
    }
    Ok(response_text)
}

fn main() -> anyhow::Result<()> {
    use bsc::led::RGB8;

    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut led = bsc::led::WS2812RMT::new()?;
    led.set_pixel(RGB8::new(50, 50, 0))?;
    let app_config = CONFIG;

    // Connect to the Wi-Fi network
    let _wifi = match bsc::wifi::wifi(app_config.wifi_ssid, app_config.wifi_psk) {
        Ok(inner) => inner,
        Err(err) => {
            // Red!
            led.set_pixel(RGB8::new(50, 0, 0))?;
            anyhow::bail!("could not connect to Wi-Fi network: {:?}", err)
        }
    };
    led.set_pixel(RGB8::new(0, 0, 50))?;

    // We're going to do all 3 devices on 1 device and later separate
    let threshold: usize = 2;
    let n_parties: usize = 2;

    let frost = Frost::new(Schnorr::<Sha256, Deterministic<Sha256>>::new(
        Deterministic::<Sha256>::default(),
    ));
    dbg!(threshold, n_parties);
    assert!(threshold <= n_parties);

    // create some scalar polynomial for each party
    let mut rng = rand::rngs::OsRng;

    println!("generating scalar polys");
    let sp = ScalarPoly::random(threshold, &mut rng);
    let sp2 = ScalarPoly::random(threshold, &mut rng);
    println!("converting to point polys");
    let pp = sp.to_point_poly();
    let pp2 = sp2.to_point_poly();

    // !! SHARE POINT POLY
    let url = CONFIG.frost_server.to_owned() + "/keygen";
    println!("{}", url);
    let response = post(url.clone(), serde_json::to_string(&pp).unwrap().as_bytes())?;
    let id: usize = serde_json::from_str(&response)?;
    let response2 = post(url.clone(), serde_json::to_string(&pp2).unwrap().as_bytes())?;
    let id2: usize = serde_json::from_str(&response2)?;
    dbg!(&pp);
    println!("Sent point poly to coordinator!");

    // !! RECEIVE POINT POLYS
    let url = CONFIG.frost_server.to_owned() + "/receive_polys";
    let response = &get(url).expect("got point polys");
    let point_polys: BTreeMap<usize, PointPoly> = serde_json::from_str(&response)?;

    let keygen = frost
        .new_keygen(point_polys.iter().map(|(_, p)| p.clone()).collect())
        .unwrap();

    // !! SEND SHARES
    println!("creating proofs of posession and shares");
    let (shares, proof_of_possesion) = frost.create_shares(&keygen, sp);
    let (shares2, proof_of_possesion2) = frost.create_shares(&keygen, sp2);
    let url = CONFIG.frost_server.to_owned() + "/send_shares";
    post(
        url.clone(),
        serde_json::to_string(&(id, shares, proof_of_possesion))
            .unwrap()
            .as_bytes(),
    )?;
    let url = CONFIG.frost_server.to_owned() + "/send_shares";
    post(
        url.clone(),
        serde_json::to_string(&(id2, shares2, proof_of_possesion2))
            .unwrap()
            .as_bytes(),
    )?;

    let url = CONFIG.frost_server.to_owned() + "/receive_shares?i=0";
    let response = &get(url).expect("got my shares");
    let (my_shares, my_pops): (Vec<Scalar>, Vec<Signature<Public>>) =
        serde_json::from_str(&response)?;
    let my_shares_zero = my_shares.into_iter().map(|s| s.mark_zero()).collect();
    let (secret_share, frost_key) =
        frost.finish_keygen_to_xonly(keygen.clone(), 0, my_shares_zero, my_pops)?;

    let url = CONFIG.frost_server.to_owned() + "/receive_shares?i=1";
    let response = &get(url).expect("got my shares");
    let (my_shares, my_pops): (Vec<Scalar>, Vec<Signature<Public>>) =
        serde_json::from_str(&response)?;
    let my_shares_zero = my_shares.into_iter().map(|s| s.mark_zero()).collect();
    let (secret_share2, frost_key2) =
        frost.finish_keygen_to_xonly(keygen, 1, my_shares_zero, my_pops)?;

    dbg!(&frost_key);
    println!("Created frost key!");

    // !! SIGNING
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
        &[sid.as_slice(), &[0]].concat(),
        Some(frost_key.public_key().normalize()),
        None,
    );
    let nonce2 = frost.gen_nonce(
        &secret_share2,
        &[sid.as_slice(), &[1]].concat(),
        Some(frost_key.public_key().normalize()),
        None,
    );

    // Send Nonces
    println!("Sharing nonces");
    let url = CONFIG.frost_server.to_owned() + "/send_nonce";
    dbg!((id, nonce.public().clone()));
    dbg!((id2, nonce2.public().clone()));
    let pub_nonce = nonce.public().clone();
    let pub_nonce2 = nonce2.public().clone();

    post(
        url.clone(),
        serde_json::to_string(&(id, pub_nonce)).unwrap().as_bytes(),
    )?;
    post(
        url.clone(),
        serde_json::to_string(&(id2, pub_nonce2))
            .unwrap()
            .as_bytes(),
    )?;

    let url = CONFIG.frost_server.to_owned() + "/receive_nonces";
    let response = &get(url).expect("got nonces");
    let nonces: Vec<(usize, Nonce)> = serde_json::from_str(&response)?;
    println!("Received nonces..");

    let msg = Message::plain("test", b"test");
    let session = frost.start_sign_session(&frost_key, nonces.clone(), msg);
    let session2 = frost.start_sign_session(&frost_key2, nonces.clone(), msg);
    let sig = frost.sign(&frost_key, &session, 0, &secret_share, nonce);
    dbg!(frost.verify_signature_share(&frost_key, &session, 0, sig));
    let sig2 = frost.sign(&frost_key2, &session2, 1, &secret_share2, nonce2);
    dbg!(frost.verify_signature_share(&frost_key2, &session2, 1, sig2));
    println!("Signed, sharing partial sigs!");

    // Send Sigs
    let url = CONFIG.frost_server.to_owned() + "/send_sig";
    post(
        url.clone(),
        serde_json::to_string(&(id, sig)).unwrap().as_bytes(),
    )?;
    post(
        url.clone(),
        serde_json::to_string(&(id2, sig2)).unwrap().as_bytes(),
    )?;

    let url = CONFIG.frost_server.to_owned() + "/receive_sigs";
    let response = &get(url).expect("get sigs");
    let sigs: Vec<(usize, Scalar)> = serde_json::from_str(&response)?;
    println!("Received signature shares..");

    // !! SUBMIT SIGS
    let combined_sig = frost.combine_signature_shares(
        &frost_key,
        &session,
        sigs.iter()
            .map(|(_, sig)| sig.clone().mark_zero().public())
            .collect(),
    );

    println!("verifying final signature");
    assert!(frost
        .schnorr
        .verify(&frost_key.public_key(), msg, &combined_sig));
    println!("Valid signature!");

    std::thread::sleep(std::time::Duration::from_secs(5));
    Ok(())
}
