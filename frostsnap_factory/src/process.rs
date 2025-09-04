use frostsnap_comms::factory::{DeviceFactorySend, FactoryDownstream, FactorySend};
use frostsnap_comms::genuine_certificate::CertificateVerifier;
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, DeviceSendBody, Direction, Downstream,
    ReceiveSerial, Sha256Digest, MAGIC_BYTES_PERIOD,
};
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun;
use frostsnap_core::schnorr_fun::fun::marker::EvenY;
use frostsnap_core::schnorr_fun::fun::Point;
use frostsnap_core::sha2;
use frostsnap_core::sha2::Sha256;
use hmac::digest::Digest;
use rand::rngs::ThreadRng;
use rand::{CryptoRng, RngCore};
use rsa::pkcs1::{DecodeRsaPublicKey, EncodeRsaPublicKey};
use rsa::RsaPublicKey;
use std::collections::HashMap;
use std::time::{self, Instant};
use tracing::*;

use crate::{ds, FactoryState};

const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;

#[derive(Debug)]
enum FactoryResult {
    Continue,
    SwitchToMain,
    FactoryComplete(String),
    GenuineComplete(String, Sha256Digest), // serial number, firmware_digest,
    FinishedAndDisconnected,
    Failed(Option<String>, String), // serial number (if known), reason
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
    BeginInitEntropy {
        provisioned_serial: String,
    },
    InitEntropy {
        provisioned_serial: String,
    },
    SettingDsKey {
        ds_public_key: RsaPublicKey,
        provisioned_serial: String,
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
        firmware_digest: Sha256Digest,
        challenge: Box<[u8; 32]>,
    },
    GenuineVerified {
        firmware_digest: Sha256Digest,
        serial: String,
    },
    // Fully finished
    AwaitingDisconnection,
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
                info!("Device connected: {}", port_desc.id);
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
                FactoryResult::GenuineComplete(serial, firmware_digest) => {
                    println!("Device {serial} passed genuine verification!");
                    if let Err(e) = factory_state.record_genuine_verified(&serial, firmware_digest)
                    {
                        error!(
                            "Failed to record genuine verification for {}: {}",
                            serial, e
                        );
                    }
                    factory_state.print_progress();

                    // We can't immediately remove the port, or it will reopen and restart process
                    // we need to wait for a disconnect.
                    // connections.remove(&port_id);

                    // Check if batch is complete
                    if factory_state.is_complete() {
                        println!("Batch complete! All devices factory setup + genuine verified");
                        std::process::exit(0);
                    }
                }
                FactoryResult::Failed(serial, reason) => {
                    println!("Device {:?} failed: {reason}", serial);
                    if let Some(serial) = serial {
                        if let Err(e) = factory_state.record_failure(&serial, &reason) {
                            error!("Failed to record failure for {}: {}", serial, e);
                        }
                    }
                    connections.remove(&port_id);
                }
                FactoryResult::FinishedAndDisconnected => {
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
                        let provisioned_serial =
                            factory_state.db.get_next_serial().unwrap().to_string();
                        connection.state = ConnectionState::BeginInitEntropy { provisioned_serial };
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
                            let _ = connection.port.write_magic_bytes().inspect_err(|_| {
                                // error!(error = e.to_string(), "failed to write magic bytes");
                            });
                        }
                    }
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::BeginInitEntropy { provisioned_serial } => {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            match connection
                .port
                .raw_send(ReceiveSerial::Message(FactorySend::InitEntropy(bytes)))
            {
                Ok(_) => {
                    connection.state = ConnectionState::InitEntropy {
                        provisioned_serial: provisioned_serial.clone(),
                    };
                }
                Err(e) => {
                    return FactoryResult::Failed(
                        Some(provisioned_serial.to_string()),
                        format!("Init entropy failed: {}", e),
                    );
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::InitEntropy { provisioned_serial } => {
            match connection.port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(DeviceFactorySend::InitEntropyOk))) => {
                    let (ds_private_key, hmac_key) = ds::generate(rng);
                    let esp32_ds_key = ds::esp32_ds_key_from_keys(&ds_private_key, hmac_key);
                    let ds_public_key = ds_private_key.to_public_key();
                    match connection.port.raw_send(ReceiveSerial::Message(
                        FactorySend::SetEsp32DsKey(esp32_ds_key),
                    )) {
                        Ok(_) => {
                            connection.state = ConnectionState::SettingDsKey {
                                ds_public_key,
                                provisioned_serial: provisioned_serial.clone(),
                            };
                        }
                        Err(e) => {
                            return FactoryResult::Failed(
                                Some(provisioned_serial.to_string()),
                                format!("DS key send failed: {}", e),
                            );
                        }
                    }
                }
                Ok(_) => {} // Keep waiting
                Err(e) => {
                    return FactoryResult::Failed(
                        Some(provisioned_serial.to_string()),
                        format!("Read error: {}", e),
                    );
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::SettingDsKey {
            ds_public_key,
            provisioned_serial,
        } => {
            match connection.port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(DeviceFactorySend::ReceivedDsKey))) => {
                    let rsa_der_bytes = ds_public_key.to_pkcs1_der().unwrap().to_vec();
                    let schnorr = schnorr_fun::new_with_synthetic_nonces::<Sha256, ThreadRng>();

                    let timestamp = time::SystemTime::now()
                        .duration_since(time::UNIX_EPOCH)
                        .expect("Time went backwards")
                        .as_secs();

                    let genuine_certificate = CertificateVerifier::sign(
                        schnorr,
                        rsa_der_bytes,
                        factory_state.target_color,
                        factory_state.revision.clone(),
                        provisioned_serial.to_string(),
                        timestamp,
                        factory_state.factory_keypair,
                    );

                    match connection.port.raw_send(ReceiveSerial::Message(
                        FactorySend::SetGenuineCertificate(genuine_certificate),
                    )) {
                        Ok(_) => {
                            connection.state = ConnectionState::FactoryDone {
                                serial: provisioned_serial.to_string(),
                            };
                        }
                        Err(e) => {
                            return FactoryResult::Failed(
                                Some(provisioned_serial.to_string()),
                                format!("Certificate send failed: {}", e),
                            );
                        }
                    }
                }
                Ok(_) => {} // Keep waiting
                Err(e) => {
                    return FactoryResult::Failed(
                        Some(provisioned_serial.to_string()),
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

    if let Err(e) = connection.port.poll_send() {
        // we only care about send failures if we're not waiting for a disconnect (conch keeps going)
        if !matches!(connection.state, ConnectionState::AwaitingDisconnection) {
            return FactoryResult::Failed(None, format!("Lost communication with device {}", e));
        }
    };

    match &connection.state {
        ConnectionState::WaitingForMainMagic { last_wrote } => {
            match connection.port.read_for_magic_bytes() {
                Ok(supported_features) => match supported_features {
                    Some(features) => {
                        connection.port.set_conch_enabled(features.conch_enabled);
                        // println!("Read device magic bytes");
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
                            // println!("Writing MAIN magic bytes");
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
                    if let Ok(DeviceSendBody::Announce { firmware_digest }) = msg.body.decode() {
                        // first, reply with announce ack
                        connection.port.queue_send(
                            CoordinatorSendMessage::to(msg.from, CoordinatorSendBody::AnnounceAck)
                                .into(),
                        );

                        let mut challenge = [0u8; 32];
                        rand::thread_rng().fill_bytes(&mut challenge);

                        connection.port.queue_send(
                            CoordinatorSendMessage::to(
                                msg.from,
                                CoordinatorSendBody::Challenge(Box::new(challenge)),
                            )
                            .into(),
                        );
                        connection.state = ConnectionState::ProcessingGenuineCheck {
                            firmware_digest,
                            challenge: Box::new(challenge),
                        };
                    }
                }
                Ok(_) => {
                    // Keep waiting
                }
                Err(e) => {
                    return FactoryResult::Failed(None, format!("Read error: {}", e));
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::ProcessingGenuineCheck {
            challenge,
            firmware_digest,
        } => {
            match connection.port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(msg))) => {
                    if let Ok(DeviceSendBody::SignedChallenge {
                        signature,
                        certificate,
                    }) = msg.body.decode()
                    {
                        let factory_key: Point<EvenY> =
                            Point::from_xonly_bytes(frostsnap_comms::FACTORY_PUBLIC_KEY).unwrap();

                        let certificate_body =
                            match CertificateVerifier::verify(&certificate, factory_key) {
                                Some(certificate_body) => certificate_body,
                                None => {
                                    return FactoryResult::Failed(
                                        Some(certificate.unverified_raw_serial()),
                                        "genuine check failed to verify!".to_string(),
                                    )
                                }
                            };
                        let serial = certificate_body.raw_serial();

                        let ds_public_key =
                            match RsaPublicKey::from_pkcs1_der(certificate_body.ds_public_key()) {
                                Ok(key) => key,
                                Err(_) => {
                                    return FactoryResult::Failed(
                                        Some(certificate_body.raw_serial()),
                                        "invalid RSA key in certificate".to_string(),
                                    );
                                }
                            };
                        let padding = rsa::Pkcs1v15Sign::new::<sha2::Sha256>();
                        let message_digest: [u8; 32] =
                            sha2::Sha256::digest(challenge.as_ref()).into();
                        match ds_public_key.verify(padding, &message_digest, signature.as_ref()) {
                            Ok(_) => {
                                connection.state = ConnectionState::GenuineVerified {
                                    firmware_digest: *firmware_digest,
                                    serial: serial.to_string(),
                                };
                            }
                            Err(_) => {
                                return FactoryResult::Failed(
                                    Some(serial.clone()),
                                    "Device failed genuine check {}!".to_string(),
                                );
                            }
                        }
                    }
                }
                Ok(_) => {} // Keep waiting
                Err(e) => {
                    return FactoryResult::Failed(None, format!("Read error: {}", e));
                }
            }
            FactoryResult::Continue
        }
        ConnectionState::GenuineVerified {
            serial,
            firmware_digest,
        } => {
            let serial = serial.clone();
            let firmware_digest = *firmware_digest;
            connection.state = ConnectionState::AwaitingDisconnection;

            FactoryResult::GenuineComplete(serial, firmware_digest)
        }
        ConnectionState::AwaitingDisconnection => {
            if connection.port.try_read_message().is_ok() {
                FactoryResult::Continue
            } else {
                FactoryResult::FinishedAndDisconnected
            }
        }
        _ => FactoryResult::Continue,
    }
}
