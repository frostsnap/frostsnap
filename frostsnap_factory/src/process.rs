use frostsnap_comms::factory::{DeviceFactorySend, FactoryDownstream, FactorySend};
use frostsnap_comms::{ReceiveSerial, MAGIC_BYTES_PERIOD};
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun::fun::hex;
use rand::{CryptoRng, RngCore};
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::RsaPublicKey;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::*;

use crate::{ds, genuine_certificate, serial_number, FactoryState, DS_CHALLENGE};

const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;

struct Connection {
    state: ConnectionState,
    port: FramedSerialPort<FactoryDownstream>,
}
enum ConnectionState {
    WaitingForMagic {
        last_wrote: Option<std::time::Instant>,
    },
    BeginInitEntropy,
    InitEntropy,
    SettingDsKey {
        rsa_pub_key: RsaPublicKey,
    },
    SavingGenuineCertificate {
        serial: String,
        rsa_pub_key: RsaPublicKey,
    },
    SigningChallenge {
        serial: String,
        rsa_pub_key: RsaPublicKey,
        challenge: Vec<u8>,
    },
    Done {
        serial: String,
    },
}

#[derive(Debug)]
enum FactoryResult {
    Success(String),        // serial number
    Failed(String, String), // serial number, reason
    Continue,
}

pub fn run_with_state(factory_state: &mut FactoryState) -> ! {
    tracing_subscriber::fmt().pretty().init();
    let serial = DesktopSerial;
    let mut rng = rand::thread_rng();
    let mut connection_state = HashMap::new();

    loop {
        for port_desc in serial.available_ports() {
            if !connection_state.contains_key(&port_desc.id)
                && port_desc.vid == USB_VID
                && port_desc.pid == USB_PID
            {
                let port = serial
                    .open_device_port(&port_desc.id, 14_000)
                    .map(FramedSerialPort::<FactoryDownstream>::new);
                match port {
                    Ok(port) => {
                        connection_state.insert(
                            port_desc.id,
                            Connection {
                                state: ConnectionState::WaitingForMagic { last_wrote: None },
                                port,
                            },
                        );
                    }
                    Err(e) => {
                        error!(
                            port = port_desc.id.to_string(),
                            error = e.to_string(),
                            "unable to open port"
                        );
                    }
                }
            }
        }

        // Process all connections and collect results
        let mut connection_results = HashMap::new();

        for (port_id, connection) in connection_state.iter_mut() {
            let result = info_span!("polling port", port = port_id.to_string())
                .in_scope(|| process_connection_state(connection, factory_state, &mut rng));
            connection_results.insert(port_id.clone(), result);
        }

        // Handle results and cleanup connections
        for (port_id, result) in connection_results {
            match result {
                FactoryResult::Success(serial) => {
                    println!("Device {serial} flashed successfully!");
                    factory_state.record_success(&serial).unwrap();
                    factory_state.print_progress();

                    // Remove completed connection
                    connection_state.remove(&port_id);

                    // Check if batch is complete
                    if factory_state.is_complete() {
                        println!(
                            "Batch complete! Processed {}/{} devices",
                            factory_state.devices_flashed, factory_state.target_quantity
                        );
                        std::process::exit(0);
                    }
                }
                FactoryResult::Failed(serial, reason) => {
                    println!("Device {serial} failed: {reason}");
                    factory_state.record_failure(&serial, &reason).unwrap();

                    // Remove failed connection
                    connection_state.remove(&port_id);
                }
                FactoryResult::Continue => {
                    // Keep processing this connection
                }
            }
        }
    }
}

fn process_connection_state(
    connection: &mut Connection,
    factory_state: &FactoryState,
    rng: &mut (impl RngCore + CryptoRng),
) -> FactoryResult {
    match &connection.state {
        ConnectionState::WaitingForMagic { last_wrote } => {
            match connection.port.read_for_magic_bytes() {
                Ok(supported_features) => match supported_features {
                    Some(_) => {
                        connection.state = ConnectionState::BeginInitEntropy;
                    }
                    None => {
                        if last_wrote.is_none()
                            || last_wrote.as_ref().unwrap().elapsed().as_millis() as u64
                                > MAGIC_BYTES_PERIOD
                        {
                            connection.state = ConnectionState::WaitingForMagic {
                                last_wrote: Some(std::time::Instant::now()),
                            };
                            let _ = connection.port.write_magic_bytes().inspect_err(|e| {
                                error!(error = e.to_string(), "failed to write magic bytes");
                            });
                        }
                    }
                },
                Err(e) => {
                    error!(error = e.to_string(), "failed to read magic bytes");
                    // Could add failure after repeated attempts
                }
            }
            FactoryResult::Continue
        }

        ConnectionState::BeginInitEntropy => {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            connection
                .port
                .raw_send(ReceiveSerial::Message(FactorySend::InitEntropy(bytes)))
                .unwrap();
            connection.state = ConnectionState::InitEntropy;
            FactoryResult::Continue
        }

        ConnectionState::InitEntropy => {
            if let Some(ReceiveSerial::Message(DeviceFactorySend::InitEntropyOk)) =
                connection.port.try_read_message().unwrap()
            {
                let (rsa_priv_key, hmac_key) = ds::generate(rng);
                let esp32_ds_key = ds::esp32_ds_key_from_keys(&rsa_priv_key, hmac_key);
                let rsa_pub_key = rsa_priv_key.to_public_key();
                connection
                    .port
                    .raw_send(ReceiveSerial::Message(FactorySend::SetEsp32DsKey(
                        esp32_ds_key,
                    )))
                    .unwrap();
                connection.state = ConnectionState::SettingDsKey { rsa_pub_key };
            }
            FactoryResult::Continue
        }

        ConnectionState::SettingDsKey { rsa_pub_key } => {
            if let Some(ReceiveSerial::Message(DeviceFactorySend::ReceivedDsKey)) =
                connection.port.try_read_message().unwrap()
            {
                let serial = serial_number::get_next().expect("serial number file should exist!");
                let genuine_certificate = genuine_certificate::generate(
                    rsa_pub_key,
                    serial,
                    factory_state.target_color.clone(),
                );
                connection
                    .port
                    .raw_send(ReceiveSerial::Message(FactorySend::SetGenuineCertificate(
                        genuine_certificate,
                    )))
                    .unwrap();
                connection.state = ConnectionState::SavingGenuineCertificate {
                    serial: serial.to_string(),
                    rsa_pub_key: rsa_pub_key.clone(),
                };
            }
            FactoryResult::Continue
        }

        ConnectionState::SavingGenuineCertificate {
            serial,
            rsa_pub_key,
        } => {
            if let Some(ReceiveSerial::Message(DeviceFactorySend::PresentGenuineCertificate(
                certificate,
            ))) = connection.port.try_read_message().unwrap()
            {
                // Verify certificate signature with factory key
                if !genuine_certificate::verify(&certificate) {
                    return FactoryResult::Failed(
                        serial.clone(),
                        "certificate signature verification failed".to_string(),
                    );
                }

                // Verify the public key matches what we expect
                let cert_pub_key = match RsaPublicKey::from_pkcs1_der(&certificate.rsa_key) {
                    Ok(key) => key,
                    Err(_) => {
                        return FactoryResult::Failed(
                            serial.clone(),
                            "invalid RSA key in certificate".to_string(),
                        );
                    }
                };

                if cert_pub_key != *rsa_pub_key {
                    return FactoryResult::Failed(
                        serial.clone(),
                        "certificate public key mismatch".to_string(),
                    );
                }

                // Challenge device to prove it has the private key
                let challenge = hex::decode(DS_CHALLENGE).unwrap();
                connection
                    .port
                    .raw_send(ReceiveSerial::Message(FactorySend::Challenge(
                        challenge.clone(),
                    )))
                    .unwrap();
                connection.state = ConnectionState::SigningChallenge {
                    serial: serial.clone(),
                    rsa_pub_key: cert_pub_key,
                    challenge,
                };
            }
            FactoryResult::Continue
        }

        ConnectionState::SigningChallenge {
            serial,
            rsa_pub_key,
            challenge,
        } => {
            if let Some(ReceiveSerial::Message(DeviceFactorySend::SignedChallenge { signature })) =
                connection.port.try_read_message().unwrap()
            {
                let message_digest: [u8; 32] = sha2::Sha256::digest(challenge).into();
                let padding = rsa::Pkcs1v15Sign::new::<Sha256>();

                match rsa_pub_key.verify(padding, &message_digest, &signature) {
                    Ok(_) => {
                        println!("Device RSA signature verification succeeded!");
                        connection.state = ConnectionState::Done {
                            serial: serial.clone(),
                        };
                    }
                    Err(_) => {
                        return FactoryResult::Failed(
                            serial.clone(),
                            "RSA signature verification failed".to_string(),
                        );
                    }
                }
            }
            FactoryResult::Continue
        }

        ConnectionState::Done { serial } => FactoryResult::Success(serial.clone()),
    }
}
