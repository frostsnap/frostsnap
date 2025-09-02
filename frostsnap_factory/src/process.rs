use frostsnap_comms::factory::{DeviceFactorySend, FactoryDownstream, FactorySend};
use frostsnap_comms::genuine_certificate::CertificateVerifier;
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, DeviceSendBody, Direction, Downstream,
    ReceiveSerial, MAGIC_BYTES_PERIOD,
};
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun;
use frostsnap_core::schnorr_fun::fun::marker::EvenY;
use frostsnap_core::schnorr_fun::fun::Point;
use frostsnap_core::sha2::Sha256;
use frostsnap_core::{sha2, sha2::Digest};
use rand::rngs::ThreadRng;
use rand::{CryptoRng, RngCore};
use rsa::pkcs1::{DecodeRsaPublicKey, EncodeRsaPublicKey};
use rsa::RsaPublicKey;
use std::collections::HashMap;
use std::time::{self, Instant};
use tracing::*;

use crate::{ds, serial_number, FactoryState};

const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;

#[derive(Debug)]
enum FactoryResult {
    Continue,
    SwitchToMain,
    FactoryComplete(String),
    GenuineComplete(String),
    Failed(String, String),
}
enum AnyConnection {
    Factory(Connection<FactoryDownstream>),
    Main(Connection<Downstream>),
}

struct Connection<T: Direction> {
    state: ConnectionState,
    port: FramedSerialPort<T>,
    last_activity: Instant,
}

enum ConnectionState {
    // Factory states
    WaitingForFactoryMagic {
        last_wrote: Option<Instant>,
        attempts: usize,
    },
    BeginInitEntropy,
    InitEntropy,
    SettingDsKey {
        ds_public_key: RsaPublicKey,
    },
    FactoryDone {
        serial: String,
    },
    // Main states
    WaitingForMainMagic {
        last_wrote: Option<Instant>,
    },
    WaitingForAnnounce,
    ProcessingGenuineCheck {
        serial: String,
        ds_public_key: RsaPublicKey,
        challenge: Box<[u8; 384]>,
    },
    GenuineVerified {
        serial: String,
    },
}

impl<T: Direction> Connection<T> {
    fn new(port: FramedSerialPort<T>, initial_state: ConnectionState) -> Self {
        Self {
            state: initial_state,
            port,
            last_activity: Instant::now(),
        }
    }
}

pub fn run_with_state(factory_state: &mut FactoryState) -> ! {
    tracing_subscriber::fmt().pretty().init();
    let desktop_serial = DesktopSerial;
    let mut rng = rand::thread_rng();
    let mut connections: HashMap<String, AnyConnection> = HashMap::new();

    loop {
        // Scan for new devices
        for port_desc in desktop_serial.available_ports() {
            if connections.contains_key(&port_desc.id) {
                continue; // Already handling this port
            }
            if port_desc.vid != USB_VID || port_desc.pid != USB_PID {
                continue;
            }

            // Start all new connections as factory - let state machine determine actual type
            if let Ok(port) = desktop_serial.open_device_port(&port_desc.id, 2000) {
                connections.insert(
                    port_desc.id.clone(),
                    AnyConnection::Factory(Connection::new(
                        FramedSerialPort::<FactoryDownstream>::new(port),
                        ConnectionState::WaitingForFactoryMagic {
                            last_wrote: None,
                            attempts: 0,
                        },
                    )),
                );
                info!("New device connected: {}", port_desc.id);
            }
        }

        // Process all connections
        let mut connection_results = HashMap::new();
        for (port_id, connection) in connections.iter_mut() {
            let result =
                info_span!("polling port", port = port_id.to_string()).in_scope(
                    || match connection {
                        AnyConnection::Factory(conn) => {
                            process_factory_connection(conn, factory_state, &mut rng)
                        }
                        AnyConnection::Main(conn) => process_main_connection(conn),
                    },
                );
            connection_results.insert(port_id.clone(), result);
        }

        // Handle results
        for (port_id, result) in connection_results {
            match result {
                FactoryResult::SwitchToMain => {
                    info!("Switching device {} to main mode", port_id);

                    // Remove factory connection and create main connection
                    connections.remove(&port_id);

                    // Wait a moment for device to settle after reboot
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    if let Ok(port) = desktop_serial.open_device_port(&port_id, 2000) {
                        connections.insert(
                            port_id,
                            AnyConnection::Main(Connection::new(
                                FramedSerialPort::<frostsnap_comms::Downstream>::new(port),
                                ConnectionState::WaitingForMainMagic { last_wrote: None },
                            )),
                        );
                        info!("Successfully created main connection");
                    } else {
                        error!("Failed to reopen port {} for main mode", port_id);
                    }
                }

                FactoryResult::FactoryComplete(serial) => {
                    println!("Device {serial} completed factory setup!");
                    if let Err(e) = factory_state.record_success(&serial) {
                        error!("Failed to record success for {}: {}", serial, e);
                    }
                    factory_state.print_progress();
                    // Remove the connection - device will reboot and reconnect as main
                    connections.remove(&port_id);
                    info!(
                        "Removed factory connection for {}, waiting for device reboot",
                        port_id
                    );
                }

                FactoryResult::GenuineComplete(serial) => {
                    println!("Device {serial} passed genuine verification!");
                    if let Err(e) = factory_state.record_genuine_verified(&serial) {
                        error!(
                            "Failed to record genuine verification for {}: {}",
                            serial, e
                        );
                    }
                    factory_state.print_progress();
                    connections.remove(&port_id);

                    // Check if batch is complete
                    if factory_state.is_complete() {
                        println!("Batch complete! All devices factory setup + genuine verified");
                        std::process::exit(0);
                    }
                }

                FactoryResult::Failed(serial, reason) => {
                    println!("Device {serial} failed: {reason}");
                    if serial != "unknown" {
                        if let Err(e) = factory_state.record_failure(&serial, &reason) {
                            error!("Failed to record failure for {}: {}", serial, e);
                        }
                    }
                    connections.remove(&port_id);
                }

                FactoryResult::Continue => {
                    // Keep processing this connection
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn process_factory_connection(
    connection: &mut Connection<FactoryDownstream>,
    factory_state: &FactoryState,
    rng: &mut (impl RngCore + CryptoRng),
) -> FactoryResult {
    connection.last_activity = Instant::now();

    match &connection.state {
        ConnectionState::WaitingForFactoryMagic {
            last_wrote,
            attempts,
        } => {
            if let Ok(supported_features) = connection.port.read_for_magic_bytes() {
                match supported_features {
                    Some(_) => {
                        connection.state = ConnectionState::BeginInitEntropy;
                    }
                    None => {
                        if last_wrote.is_none()
                            || last_wrote.as_ref().unwrap().elapsed().as_millis() as u64
                                > MAGIC_BYTES_PERIOD
                        {
                            if *attempts > 5 {
                                return FactoryResult::SwitchToMain;
                            }
                            connection.state = ConnectionState::WaitingForFactoryMagic {
                                last_wrote: Some(Instant::now()),
                                attempts: *attempts + 1,
                            };
                            println!("Writing FACTORY magic bytes");
                            let _ = connection.port.write_magic_bytes().inspect_err(|_| {
                                // error!(error = e.to_string(), "failed to write magic bytes");
                            });
                        }
                    }
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::BeginInitEntropy => {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            match connection
                .port
                .raw_send(ReceiveSerial::Message(FactorySend::InitEntropy(bytes)))
            {
                Ok(_) => {
                    connection.state = ConnectionState::InitEntropy;
                }
                Err(e) => {
                    return FactoryResult::Failed(
                        "unknown".to_string(),
                        format!("Init entropy failed: {}", e),
                    );
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::InitEntropy => {
            match connection.port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(DeviceFactorySend::InitEntropyOk))) => {
                    let (ds_private_key, hmac_key) = ds::generate(rng);
                    let esp32_ds_key = ds::esp32_ds_key_from_keys(&ds_private_key, hmac_key);
                    let ds_public_key = ds_private_key.to_public_key();
                    match connection.port.raw_send(ReceiveSerial::Message(
                        FactorySend::SetEsp32DsKey(esp32_ds_key),
                    )) {
                        Ok(_) => {
                            connection.state = ConnectionState::SettingDsKey { ds_public_key };
                        }
                        Err(e) => {
                            return FactoryResult::Failed(
                                "unknown".to_string(),
                                format!("DS key send failed: {}", e),
                            );
                        }
                    }
                }
                Ok(_) => {} // Keep waiting
                Err(e) => {
                    return FactoryResult::Failed(
                        "unknown".to_string(),
                        format!("Read error: {}", e),
                    );
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::SettingDsKey { ds_public_key } => {
            match connection.port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(DeviceFactorySend::ReceivedDsKey))) => {
                    match serial_number::get_next() {
                        Ok(device_serial) => {
                            let rsa_der_bytes = ds_public_key.to_pkcs1_der().unwrap().to_vec();
                            let schnorr =
                                schnorr_fun::new_with_synthetic_nonces::<Sha256, ThreadRng>();

                            let timestamp = time::SystemTime::now()
                                .duration_since(time::UNIX_EPOCH)
                                .expect("Time went backwards")
                                .as_secs();

                            let genuine_certificate = CertificateVerifier::sign(
                                schnorr,
                                rsa_der_bytes,
                                factory_state.target_color,
                                factory_state.revision.clone(),
                                device_serial.to_string(),
                                timestamp,
                                factory_state.factory_keypair,
                            );

                            match connection.port.raw_send(ReceiveSerial::Message(
                                FactorySend::SetGenuineCertificate(genuine_certificate),
                            )) {
                                Ok(_) => {
                                    connection.state = ConnectionState::FactoryDone {
                                        serial: device_serial.to_string(),
                                    };
                                }
                                Err(e) => {
                                    return FactoryResult::Failed(
                                        device_serial.to_string(),
                                        format!("Certificate send failed: {}", e),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            return FactoryResult::Failed(
                                "unknown".to_string(),
                                format!("Serial generation failed: {}", e),
                            );
                        }
                    }
                }
                Ok(_) => {} // Keep waiting
                Err(e) => {
                    return FactoryResult::Failed(
                        "unknown".to_string(),
                        format!("Read error: {}", e),
                    );
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::FactoryDone { serial } => FactoryResult::FactoryComplete(serial.clone()),
        _ => FactoryResult::Continue,
    }
}

fn process_main_connection(connection: &mut Connection<Downstream>) -> FactoryResult {
    connection.last_activity = Instant::now();

    connection.port.poll_send().unwrap();

    match &connection.state {
        ConnectionState::WaitingForMainMagic { last_wrote } => {
            match connection.port.read_for_magic_bytes() {
                Ok(supported_features) => match supported_features {
                    Some(features) => {
                        connection.port.set_conch_enabled(features.conch_enabled);
                        println!("Read device magic bytes");
                        connection.state = ConnectionState::WaitingForAnnounce;
                        FactoryResult::Continue
                    }
                    None => {
                        if last_wrote.is_none()
                            || last_wrote.as_ref().unwrap().elapsed().as_millis() as u64
                                > MAGIC_BYTES_PERIOD
                        {
                            connection.state = ConnectionState::WaitingForMainMagic {
                                last_wrote: Some(Instant::now()),
                            };
                            println!("Writing MAIN magic bytes");
                            let _ = connection.port.write_magic_bytes().inspect_err(|e| {
                                error!(error = e.to_string(), "failed to write main magic bytes");
                            });
                        }
                        FactoryResult::Continue
                    }
                },
                Err(e) => {
                    error!(error = e.to_string(), "failed to read main magic bytes");
                    FactoryResult::Continue
                }
            }
        }
        ConnectionState::WaitingForAnnounce => {
            match connection.port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(msg))) => {
                    if let Ok(DeviceSendBody::Announce {
                        genuine_cert: Some(genuine_cert),
                        ..
                    }) = msg.body.decode()
                    {
                        let factory_key: Point<EvenY> =
                            Point::from_xonly_bytes(frostsnap_comms::FACTORY_PUBLIC_KEY).unwrap();

                        let certifcate_body =
                            match CertificateVerifier::verify(&genuine_cert, factory_key) {
                                Some(certifcate_body) => certifcate_body,
                                None => {
                                    return FactoryResult::Failed(
                                        genuine_cert.unverified_serial_number(),
                                        "certificate signature verification failed".to_string(),
                                    );
                                }
                            };

                        let ds_public_key =
                            match RsaPublicKey::from_pkcs1_der(certifcate_body.ds_public_key()) {
                                Ok(key) => key,
                                Err(_) => {
                                    return FactoryResult::Failed(
                                        certifcate_body.serial_number(),
                                        "invalid RSA key in certificate".to_string(),
                                    );
                                }
                            };

                        let mut challenge = [0u8; 384];
                        rand::thread_rng().fill_bytes(&mut challenge);

                        connection.port.queue_send(
                            CoordinatorSendMessage::to(
                                msg.from,
                                CoordinatorSendBody::Challenge(Box::new(challenge)),
                            )
                            .into(),
                        );
                        connection.state = ConnectionState::ProcessingGenuineCheck {
                            serial: certifcate_body.serial_number(),
                            ds_public_key,
                            challenge: Box::new(challenge),
                        };
                    }
                }
                Ok(_) => {
                    // Keep waiting
                }
                Err(e) => {
                    return FactoryResult::Failed(
                        "unknown".to_string(),
                        format!("Read error: {}", e),
                    );
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::ProcessingGenuineCheck {
            serial,
            ds_public_key,
            challenge,
        } => {
            match connection.port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(msg))) => {
                    if let Ok(DeviceSendBody::SignedChallenge { signature }) = msg.body.decode() {
                        let message_digest: [u8; 32] = sha2::Sha256::digest(**challenge).into();
                        let padding = rsa::Pkcs1v15Sign::new::<sha2::Sha256>();
                        match ds_public_key.verify(padding, &message_digest, signature.as_ref()) {
                            Ok(_) => {
                                connection.state = ConnectionState::GenuineVerified {
                                    serial: serial.to_string(),
                                };
                            }
                            Err(_) => {
                                return FactoryResult::Failed(
                                    serial.clone(),
                                    "Device failed genuine check!".to_string(),
                                );
                            }
                        }
                    }
                }
                Ok(_) => {} // Keep waiting
                Err(e) => {
                    return FactoryResult::Failed(serial.clone(), format!("Read error: {}", e));
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::GenuineVerified { serial } => {
            FactoryResult::GenuineComplete(serial.clone())
        }
        _ => FactoryResult::Continue,
    }
}
