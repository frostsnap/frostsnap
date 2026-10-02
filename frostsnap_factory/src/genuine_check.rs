use frostsnap_comms::genuine_certificate::{self, CaseColor, CertificateBody, DsSignature};
use frostsnap_comms::genuine_check::{
    self, AttestedDevice, AttestedDeviceDigest, CoordinatorMessage, DeviceMessage,
};
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, DeviceSendBody, Downstream, GenuineChallenge,
    ReceiveSerial, Sha256Digest, MAGIC_BYTES_PERIOD,
};
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use frostsnap_core::schnorr_fun::Signature;
use frostsnap_core::{DeviceId, Gist};
use std::time::Instant;

use crate::{USB_PID, USB_VID};

pub enum GenuineCheckState {
    WaitingForMagic {
        last_wrote: Option<Instant>,
    },
    WaitingForAnnounce,
    AwaitingFactoryAttestation {
        firmware_digest: Sha256Digest,
        challenge: GenuineChallenge,
    },
    AwaitingIdentityAttestation {
        firmware_digest: Sha256Digest,
        challenge: GenuineChallenge,
        attested_digest: AttestedDeviceDigest,
        body: Box<CertificateBody>,
    },
    Complete {
        firmware_digest: Sha256Digest,
        serial: String,
    },
    AwaitingDisconnection,
    Disconnected,
}

pub enum GenuineCheckPollResult {
    Continue,
    Verified {
        serial: String,
        firmware_digest: Sha256Digest,
    },
    Disconnected,
    Failed(Option<String>, String),
}

/// Poll one step of the genuine check state machine.
/// Returns `Continue` if more polling is needed.
pub fn poll_genuine_check(
    port: &mut FramedSerialPort<Downstream>,
    state: &mut GenuineCheckState,
    genuine_key: Point<EvenY>,
) -> GenuineCheckPollResult {
    if let Err(e) = port.poll_send() {
        if !matches!(state, GenuineCheckState::AwaitingDisconnection) {
            return GenuineCheckPollResult::Failed(
                None,
                format!("Lost communication with device: {e}"),
            );
        }
    }

    match state {
        GenuineCheckState::WaitingForMagic { last_wrote } => {
            match port.read_for_magic_bytes() {
                Ok(Some(features)) => {
                    port.set_conch_enabled(features.conch_enabled);
                    *state = GenuineCheckState::WaitingForAnnounce;
                }
                Ok(None) => {
                    if last_wrote.is_none()
                        || last_wrote.unwrap().elapsed().as_millis() as u64 > MAGIC_BYTES_PERIOD
                    {
                        let _ = port.write_magic_bytes();
                        *last_wrote = Some(Instant::now());
                    }
                }
                Err(_) => {}
            }
            GenuineCheckPollResult::Continue
        }
        GenuineCheckState::WaitingForAnnounce => {
            match port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(msg))) => {
                    if let Ok(DeviceSendBody::Announce { firmware_digest }) = msg.body.decode() {
                        port.queue_send(
                            CoordinatorSendMessage::to(msg.from, CoordinatorSendBody::AnnounceAck)
                                .into(),
                        );

                        let challenge = GenuineChallenge::random(&mut rand::thread_rng());
                        port.queue_send(
                            CoordinatorSendMessage::to(
                                msg.from,
                                CoordinatorSendBody::GenuineCheck(
                                    CoordinatorMessage::RequestFactoryAttestation { challenge },
                                ),
                            )
                            .into(),
                        );
                        *state = GenuineCheckState::AwaitingFactoryAttestation {
                            firmware_digest,
                            challenge,
                        };
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    return GenuineCheckPollResult::Failed(None, format!("Read error: {e}"));
                }
            }
            GenuineCheckPollResult::Continue
        }
        GenuineCheckState::AwaitingFactoryAttestation {
            firmware_digest,
            challenge,
        } => {
            let (from, message) = match read_genuine_message(port) {
                Ok(Some(received)) => received,
                Ok(None) => return GenuineCheckPollResult::Continue,
                Err(e) => return GenuineCheckPollResult::Failed(None, e),
            };
            let DeviceMessage::FactoryAttestation {
                attested,
                ds_signature,
            } = message
            else {
                return GenuineCheckPollResult::Failed(
                    None,
                    "identity attestation arrived before the factory attestation".to_string(),
                );
            };
            match genuine_check::verify_factory_attestation(
                &attested,
                genuine_key,
                *challenge,
                from,
                &ds_signature,
            ) {
                Ok(body) => {
                    let challenge = GenuineChallenge::random(&mut rand::thread_rng());
                    let attested_digest = attested.digest();
                    port.queue_send(
                        CoordinatorSendMessage::to(
                            from,
                            CoordinatorSendBody::GenuineCheck(
                                CoordinatorMessage::RequestIdentityAttestation {
                                    challenge,
                                    attested_digest,
                                },
                            ),
                        )
                        .into(),
                    );
                    *state = GenuineCheckState::AwaitingIdentityAttestation {
                        firmware_digest: *firmware_digest,
                        challenge,
                        attested_digest,
                        body: Box::new(body),
                    };
                    GenuineCheckPollResult::Continue
                }
                Err(e) => GenuineCheckPollResult::Failed(
                    Some(attested.certificate().unverified_raw_serial()),
                    format!("Device failed genuine check: {e}"),
                ),
            }
        }
        GenuineCheckState::AwaitingIdentityAttestation {
            firmware_digest,
            challenge,
            attested_digest,
            body,
        } => {
            let (from, message) = match read_genuine_message(port) {
                Ok(Some(received)) => received,
                Ok(None) => return GenuineCheckPollResult::Continue,
                Err(e) => return GenuineCheckPollResult::Failed(None, e),
            };
            let DeviceMessage::IdentityAttestation { signature } = message else {
                return GenuineCheckPollResult::Failed(
                    Some(body.raw_serial()),
                    "device made a factory attestation instead of an identity attestation"
                        .to_string(),
                );
            };
            if let Err(e) = genuine_check::verify_identity_attestation(
                from,
                *challenge,
                *attested_digest,
                &signature,
            ) {
                return GenuineCheckPollResult::Failed(
                    Some(body.raw_serial()),
                    format!("Device failed genuine check: {e}"),
                );
            }
            *state = GenuineCheckState::Complete {
                firmware_digest: *firmware_digest,
                serial: body.raw_serial(),
            };
            GenuineCheckPollResult::Continue
        }
        GenuineCheckState::Complete {
            serial,
            firmware_digest,
        } => {
            let result = GenuineCheckPollResult::Verified {
                serial: serial.clone(),
                firmware_digest: *firmware_digest,
            };
            *state = GenuineCheckState::AwaitingDisconnection;
            result
        }
        GenuineCheckState::AwaitingDisconnection => {
            if port.try_read_message().is_ok() {
                GenuineCheckPollResult::Continue
            } else {
                *state = GenuineCheckState::Disconnected;
                GenuineCheckPollResult::Disconnected
            }
        }
        GenuineCheckState::Disconnected => GenuineCheckPollResult::Disconnected,
    }
}

fn wait_for_magic(
    port: &mut FramedSerialPort<Downstream>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_wrote: Option<Instant> = None;
    loop {
        match port.read_for_magic_bytes() {
            Ok(Some(features)) => {
                port.set_conch_enabled(features.conch_enabled);
                return Ok(());
            }
            Ok(None) => {
                if last_wrote.is_none()
                    || last_wrote.unwrap().elapsed().as_millis() as u64 > MAGIC_BYTES_PERIOD
                {
                    port.write_magic_bytes()?;
                    last_wrote = Some(Instant::now());
                }
            }
            Err(e) => return Err(format!("Failed to read magic bytes: {e}").into()),
        }
    }
}

fn wait_for_announce(
    port: &mut FramedSerialPort<Downstream>,
) -> Result<(DeviceId, Sha256Digest), Box<dyn std::error::Error>> {
    loop {
        match port.try_read_message() {
            Ok(Some(ReceiveSerial::Message(msg))) => {
                if let Ok(DeviceSendBody::Announce { firmware_digest }) = msg.body.decode() {
                    return Ok((msg.from, firmware_digest));
                }
            }
            Ok(_) => {}
            Err(e) => return Err(format!("Read error: {e}").into()),
        }
    }
}

fn read_genuine_message(
    port: &mut FramedSerialPort<Downstream>,
) -> Result<Option<(DeviceId, DeviceMessage)>, String> {
    match port.try_read_message() {
        Ok(Some(ReceiveSerial::Message(msg))) => match msg.body.decode() {
            Ok(DeviceSendBody::GenuineCheck(message)) => Ok(Some((msg.from, message))),
            Ok(other) => {
                tracing::debug!(
                    device = msg.from.to_string(),
                    gist = other.gist(),
                    "ignoring a message that isn't part of the genuine check"
                );
                Ok(None)
            }
            Err(e) => Err(format!(
                "device {} sent a message that failed to decode: {e}",
                msg.from
            )),
        },
        Ok(_) => Ok(None),
        Err(e) => Err(format!("Read error: {e}")),
    }
}

fn wait_for_factory_attestation(
    port: &mut FramedSerialPort<Downstream>,
) -> Result<(AttestedDevice, DsSignature), Box<dyn std::error::Error>> {
    loop {
        port.poll_send()?;
        match read_genuine_message(port)? {
            Some((
                _,
                DeviceMessage::FactoryAttestation {
                    attested,
                    ds_signature,
                },
            )) => return Ok((*attested, *ds_signature)),
            Some((_, DeviceMessage::IdentityAttestation { .. })) => {
                return Err("identity attestation arrived before the factory attestation".into())
            }
            None => {}
        }
    }
}

fn wait_for_identity_attestation(
    port: &mut FramedSerialPort<Downstream>,
) -> Result<Signature, Box<dyn std::error::Error>> {
    loop {
        port.poll_send()?;
        match read_genuine_message(port)? {
            Some((_, DeviceMessage::IdentityAttestation { signature })) => return Ok(signature),
            Some((_, DeviceMessage::FactoryAttestation { .. })) => {
                return Err(
                    "device made a factory attestation instead of an identity attestation".into(),
                )
            }
            None => {}
        }
    }
}

fn find_factory_key<'a>(
    certificate: &genuine_certificate::Certificate,
    known_keys: &[(&'a str, Point<EvenY>)],
) -> Result<(&'a str, Point<EvenY>), Box<dyn std::error::Error>> {
    known_keys
        .iter()
        .find(|(_, key)| genuine_certificate::verify_certificate(certificate, *key).is_some())
        .copied()
        .ok_or_else(|| "Certificate not signed by any known genuine key".into())
}

pub struct GenuineCheckResult {
    pub serial: String,
    pub color: CaseColor,
    pub revision: String,
    pub timestamp: u64,
    pub firmware_digest: Sha256Digest,
    pub env: String,
}

pub fn run_genuine_check(
    known_keys: &[(&str, Point<EvenY>)],
) -> Result<GenuineCheckResult, Box<dyn std::error::Error>> {
    let desktop_serial = DesktopSerial;

    println!("Waiting for device...");
    let mut port: FramedSerialPort<Downstream> = loop {
        let found = desktop_serial
            .available_ports()
            .into_iter()
            .find(|p| p.vid == USB_VID && p.pid == USB_PID)
            .and_then(|p| desktop_serial.open_device_port(&p.id, 2000).ok());
        if let Some(port) = found {
            println!("Device connected");
            break FramedSerialPort::<Downstream>::new(port);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    println!("Exchanging magic bytes...");
    wait_for_magic(&mut port)?;

    println!("Waiting for device announce...");
    let (device_id, firmware_digest) = wait_for_announce(&mut port)?;

    println!("Requesting factory attestation...");
    port.queue_send(CoordinatorSendMessage::to(device_id, CoordinatorSendBody::AnnounceAck).into());
    let challenge = GenuineChallenge::random(&mut rand::thread_rng());
    port.queue_send(
        CoordinatorSendMessage::to(
            device_id,
            CoordinatorSendBody::GenuineCheck(CoordinatorMessage::RequestFactoryAttestation {
                challenge,
            }),
        )
        .into(),
    );
    let (attested, ds_signature) = wait_for_factory_attestation(&mut port)?;
    let (env_name, factory_key) = find_factory_key(attested.certificate(), known_keys)?;
    let certificate_body = genuine_check::verify_factory_attestation(
        &attested,
        factory_key,
        challenge,
        device_id,
        &ds_signature,
    )?;

    println!("Requesting identity attestation...");
    let challenge = GenuineChallenge::random(&mut rand::thread_rng());
    let attested_digest = attested.digest();
    port.queue_send(
        CoordinatorSendMessage::to(
            device_id,
            CoordinatorSendBody::GenuineCheck(CoordinatorMessage::RequestIdentityAttestation {
                challenge,
                attested_digest,
            }),
        )
        .into(),
    );
    let identity_signature = wait_for_identity_attestation(&mut port)?;
    genuine_check::verify_identity_attestation(
        device_id,
        challenge,
        attested_digest,
        &identity_signature,
    )?;

    let CertificateBody::Frontier {
        case_color,
        revision,
        serial,
        timestamp,
        ..
    } = &certificate_body;

    Ok(GenuineCheckResult {
        serial: serial.clone(),
        color: *case_color,
        revision: revision.clone(),
        timestamp: *timestamp,
        firmware_digest,
        env: env_name.to_string(),
    })
}
