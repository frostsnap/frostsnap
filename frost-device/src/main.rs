use std::collections::BTreeMap;

use esp32_c3_dkc02_bsc as bsc;
// Types for button interupt
use esp_idf_sys::{
    self as _, c_types::c_void, esp, gpio_config, gpio_config_t, gpio_install_isr_service,
    gpio_int_type_t_GPIO_INTR_POSEDGE, gpio_isr_handler_add, gpio_mode_t_GPIO_MODE_INPUT,
    xQueueGenericCreate, xQueueGiveFromISR, xQueueReceive, QueueHandle_t, ESP_INTR_FLAG_IRAM,
};
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

// This `static mut` holds the queue handle we are going to get from `xQueueGenericCreate`.
// This is unsafe, but we are careful not to enable our GPIO interrupt handler until after this value has been initialised, and then never modify it again
static mut EVENT_QUEUE: Option<QueueHandle_t> = None;

#[link_section = ".iram0.text"]
unsafe extern "C" fn button_interrupt(_: *mut c_void) {
    xQueueGiveFromISR(EVENT_QUEUE.unwrap(), std::ptr::null_mut());
}

fn get(url: impl AsRef<str>) -> anyhow::Result<String> {
    let mut client = EspHttpClient::new_default()?;
    let request = client.get(url.as_ref())?;

    let writer = request.into_writer(0)?;

    let mut response = writer.submit()?;
    let status = response.status();
    let mut total_size = 0;

    // println!("response code: {}\n", status);

    let mut response_text = "".to_string();
    match status {
        200..=299 => {
            let mut buf = [0_u8; 256];
            let mut reader = response.reader();
            let _ = loop {
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
    let mut _total_size = 0;

    // println!("response code: {}\n", status);

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
                    _total_size += size;
                    // 6. try converting the bytes into a Rust (UTF-8) string and print it
                    response_text += &std::str::from_utf8(&buf[..size])?.to_string();
                }
            }
        }
        _ => anyhow::bail!("unexpected response code: {}", status),
    }
    Ok(response_text)
}

fn loop_until_press() {
    loop {
        unsafe {
            // maximum delay
            const QUEUE_WAIT_TICKS: u32 = 1000;

            // Reads the event item out of the queue
            let res = xQueueReceive(EVENT_QUEUE.unwrap(), std::ptr::null_mut(), QUEUE_WAIT_TICKS);

            // If the event has the value 0, nothing happens. if it has a different value, the button was pressed.
            match res {
                1 => {
                    // println!("continuing..");
                    break;
                }
                _ => {}
            };
        }
    }
}

fn prompt_wait(s: &str) {
    println!(" ");
    println!("Press button to {}:", s);
    loop_until_press();
}

fn main() -> anyhow::Result<()> {
    // Button interupt
    {
        const GPIO_NUM: i32 = 9;

        // Configures the button
        let io_conf = gpio_config_t {
            pin_bit_mask: 1 << GPIO_NUM,
            mode: gpio_mode_t_GPIO_MODE_INPUT,
            pull_up_en: true.into(),
            pull_down_en: false.into(),
            intr_type: gpio_int_type_t_GPIO_INTR_POSEDGE, // positive edge trigger = button down
        };

        // Queue configurations
        const QUEUE_TYPE_BASE: u8 = 0;
        const ITEM_SIZE: u32 = 0; // we're not posting any actual data, just notifying
        const QUEUE_SIZE: u32 = 1;

        unsafe {
            // Writes the button configuration to the registers
            esp!(gpio_config(&io_conf))?;

            // Installs the generic GPIO interrupt handler
            esp!(gpio_install_isr_service(ESP_INTR_FLAG_IRAM as i32))?;

            // Instantiates the event queue
            EVENT_QUEUE = Some(xQueueGenericCreate(QUEUE_SIZE, ITEM_SIZE, QUEUE_TYPE_BASE));

            // Registers our function with the generic GPIO interrupt handler we installed earlier.
            esp!(gpio_isr_handler_add(
                GPIO_NUM,
                Some(button_interrupt),
                std::ptr::null_mut()
            ))?;
        }
    }

    use bsc::led::RGB8;

    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // WIFI stuff
    let mut led = bsc::led::WS2812RMT::new()?;
    led.set_pixel(RGB8::new(10, 25, 0))?;
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

    // Clear server state
    let url = CONFIG.frost_server.to_owned() + "/clear";
    match get(url) {
        Err(err) => {
            led.set_pixel(RGB8::new(50, 0, 0))?;
            anyhow::bail!(
                "could not connect to FROST coordinating server: {} {:?}",
                CONFIG.frost_server,
                err
            )
        }
        Ok(_) => {}
    };

    // FROST
    //
    // We're going to carry out 2 parties on 1 device and later separate
    let threshold: usize = 2;
    let n_parties: usize = 3;

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
    dbg!(&pp);

    // let sp2 = ScalarPoly::random(threshold, &mut rng);
    // let pp2 = sp2.to_point_poly();

    // Share point polynomials
    let url = CONFIG.frost_server.to_owned() + "/keygen";
    let response = post(
        url.clone(),
        serde_json::to_string(&(threshold, n_parties, pp))
            .unwrap()
            .as_bytes(),
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
    led.set_pixel(RGB8::new(25, 25, 50))?;

    // Send shares
    let (shares, proof_of_possesion) = frost.create_shares(&keygen, sp);
    dbg!(&shares);
    let url = CONFIG.frost_server.to_owned() + "/send_shares";
    post(
        url.clone(),
        serde_json::to_string(&(id, shares, proof_of_possesion))
            .unwrap()
            .as_bytes(),
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
    led.set_pixel(RGB8::new(10, 10, 50))?;

    // Signing

    println!("|\n| SIGNING \n|");
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
        serde_json::to_string(&(id, pub_nonce)).unwrap().as_bytes(),
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
    let msg = Message::plain("test", b"test");
    let session = frost.start_sign_session(&frost_key, nonces.clone(), msg);
    let sig = frost.sign(&frost_key, &session, id, &secret_share, nonce);
    dbg!(&sig);
    println!("Signed, sharing partial sigs!");

    // let session2 = frost.start_sign_session(&frost_key2, nonces.clone(), msg);
    // dbg!(frost.verify_signature_share(&frost_key, &session, 0, sig));
    // let sig2 = frost.sign(&frost_key2, &session2, 1, &secret_share2, nonce2);

    // Send Sigs
    let url = CONFIG.frost_server.to_owned() + "/send_sig";
    post(
        url.clone(),
        serde_json::to_string(&(id, sig)).unwrap().as_bytes(),
    )?;

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
        loop {
            led.set_pixel(RGB8::new(10, 10, 50))?;
            std::thread::sleep(std::time::Duration::from_secs(1));
            led.set_pixel(RGB8::new(10, 50, 10))?;
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    } else {
        println!("Invalid signature :(");
        led.set_pixel(RGB8::new(50, 0, 0))?;
    }
    Ok(())
}
