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

use schnorr_fun::fun::digest::typenum::Zero;
use schnorr_fun::Signature;
use schnorr_fun::{
    frost::{Frost, PointPoly, ScalarPoly, XOnlyFrostKey},
    fun::{marker::Public, Scalar},
    musig::NonceKeyPair,
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

fn post(url: impl AsRef<str>, data: &[u8]) -> anyhow::Result<()> {
    let mut client = EspHttpClient::new_default()?;
    let request = client.post(url.as_ref())?;

    let mut writer = request.into_writer(data.len())?;
    writer.write(data)?;

    let mut response = writer.submit()?;
    let status = response.status();
    let mut total_size = 0;

    println!("response code: {}\n", status);

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
                    let response_text = std::str::from_utf8(&buf[..size])?;
                    println!("{}", response_text);
                }
            }
        }
        _ => anyhow::bail!("unexpected response code: {}", status),
    }
    println!("{}", &total_size);
    Ok(())
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
    // Blue!
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
    post(url.clone(), serde_json::to_string(&pp).unwrap().as_bytes())?;
    post(url.clone(), serde_json::to_string(&pp2).unwrap().as_bytes())?;
    dbg!(&pp);
    println!("Sent point poly to coordinator!");

    // !! RECEIVE POINT POLYS
    let url = CONFIG.frost_server.to_owned() + "/receive_polys";
    let response = &get(url).expect("got point polys");
    let point_polys: Vec<PointPoly> = serde_json::from_str(&response)?;

    let keygen = frost.new_keygen(point_polys).unwrap();

    // !! SEND SHARES
    println!("creating proofs of posession and shares");
    let (shares, proof_of_possesion) = frost.create_shares(&keygen, sp);
    let (shares2, proof_of_possesion2) = frost.create_shares(&keygen, sp2);
    let url = CONFIG.frost_server.to_owned() + "/send_shares";
    dbg!(&url);
    post(
        url.clone(),
        serde_json::to_string(&(shares, proof_of_possesion))
            .unwrap()
            .as_bytes(),
    )?;
    post(
        url.clone(),
        serde_json::to_string(&(shares2, proof_of_possesion2))
            .unwrap()
            .as_bytes(),
    )?;

    let url = CONFIG.frost_server.to_owned() + "/receive_shares?i=0";
    dbg!(&url);
    let response = &get(url).expect("got my shares");
    let (my_shares, my_pops): (Vec<Scalar>, Vec<Signature<Public>>) =
        serde_json::from_str(&response)?;
    let my_shares_zero = my_shares.into_iter().map(|s| s.mark_zero()).collect();
    let (secret_share, frost_key) = frost.finish_keygen(keygen, 0, my_shares_zero, my_pops)?;
    dbg!(frost_key);

    println!("Created frost key!");

    // !! SEND SECRET SHARES

    // for party_index in 0..n_parties {
    //     println!("Collecting shares for {}", party_index);
    //     recieved_shares.push(vec![]);
    //     for share_index in 0..n_parties {
    //         recieved_shares[party_index].push(shares[share_index][party_index].clone());
    //     }
    // }

    // // println!("{:?}", recieved_shares);

    // // finish keygen for each party
    // let (secret_shares, frost_keys): (Vec<Scalar>, Vec<XOnlyFrostKey>) = (0..n_parties)
    //     .map(|i| {
    //         println!("Finishing keygen for participant {}", i);
    //         let res = frost.finish_keygen(
    //             keygen.clone(),
    //             i,
    //             recieved_shares[i].clone(),
    //             proofs_of_possesion.clone(),
    //         );
    //         match res.clone() {
    //             Err(e) => {
    //                 println!("{:?}", e)
    //             }
    //             Ok(_) => {}
    //         }

    //         let (secret_share, frost_key) = res.unwrap();

    //         println!("got secret share");
    //         let xonly_frost_key = frost_key.into_xonly_key();
    //         (secret_share, xonly_frost_key)
    //     })
    //     .unzip();
    // println!("Finished keygen.");

    // println!("Selecting signers...");

    // // use a boolean mask for which t participants are signers
    // let mut signer_mask = vec![true; threshold];
    // signer_mask.append(&mut vec![false; n_parties - threshold]);
    // // shuffle the mask for random signers

    // let signer_indexes: Vec<_> = signer_mask
    //     .iter()
    //     .enumerate()
    //     .filter(|(_, is_signer)| **is_signer)
    //     .map(|(i, _)| i)
    //     .collect();

    // println!("Preparing for signing session...");

    // let verification_shares_bytes: Vec<_> = frost_keys[signer_indexes[0]]
    //     .verification_shares()
    //     .map(|share| share.to_bytes())
    //     .collect();

    // let sid = [
    //     frost_keys[signer_indexes[0]]
    //         .public_key()
    //         .to_xonly_bytes()
    //         .as_slice(),
    //     verification_shares_bytes.concat().as_slice(),
    //     b"frost-prop-test".as_slice(),
    // ]
    // .concat();
    // let nonces: Vec<NonceKeyPair> = signer_indexes
    //     .iter()
    //     .map(|i| {
    //         frost.gen_nonce(
    //             &secret_shares[*i],
    //             &[sid.as_slice(), [*i as u8].as_slice()].concat(),
    //             Some(frost_keys[signer_indexes[0]].public_key().normalize()),
    //             None,
    //         )
    //     })
    //     .collect();

    // let mut recieved_nonces: Vec<_> = vec![];
    // for (i, nonce) in signer_indexes.iter().zip(nonces.clone()) {
    //     recieved_nonces.push((*i, nonce.public()));
    // }
    // println!("Recieved nonces..");

    // // Create Frost signing session
    // let signing_session = frost.start_sign_session(
    //     &frost_keys[signer_indexes[0]],
    //     recieved_nonces.clone(),
    //     Message::plain("test", b"test"),
    // );

    // // let mut signatures = vec![];
    // // for i in 0..signer_indexes.len() {

    // // }
    // let signatures = (0..signer_indexes.len())
    //     .map(|i| {
    //         println!("Signing for participant {}", signer_indexes[i]);
    //         let signer_index = signer_indexes[i];
    //         let session = frost.start_sign_session(
    //             &frost_keys[signer_index],
    //             recieved_nonces.clone(),
    //             Message::plain("test", b"test"),
    //         );
    //         let sig = frost.sign(
    //             &frost_keys[signer_index],
    //             &session,
    //             signer_index,
    //             &secret_shares[signer_index],
    //             nonces[i].clone(),
    //         );
    //         assert!(frost.verify_signature_share(
    //             &frost_keys[signer_index],
    //             &session,
    //             signer_index,
    //             sig
    //         ));
    //         sig
    //     })
    //     .collect();
    // let combined_sig = frost.combine_signature_shares(
    //     &frost_keys[signer_indexes[0]],
    //     &signing_session,
    //     signatures,
    // );

    // println!("verifying final signature");
    // assert!(frost.schnorr.verify(
    //     &frost_keys[signer_indexes[0]].public_key(),
    //     Message::<Public>::plain("test", b"test"),
    //     &combined_sig
    // ));
    // println!("Valid signature!");

    // drop(nonces);
    // drop(recieved_nonces);
    std::thread::sleep(std::time::Duration::from_secs(5));
    Ok(())
}
