use frostsnap_comms::genuine_certificate::{self, CaseColor, CertificateBody};
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, DeviceSendBody, Downstream, GenuineChallenge,
    ReceiveSerial, Sha256Digest, MAGIC_BYTES_PERIOD,
};
use frostsnap_coordinator::frostsnap_persist::Attestation;
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use frostsnap_core::schnorr_fun::Signature;
use std::time::Instant;

use crate::{USB_PID, USB_VID};

pub enum GenuineCheckState {
    WaitingForMagic {
        last_wrote: Option<Instant>,
    },
    WaitingForAnnounce,
    ProcessingChallenge {
        firmware_digest: Sha256Digest,
        challenge: GenuineChallenge,
        attested: Option<CertificateBody>,
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
                        queue_genuine_requests(port, msg.from, challenge);
                        *state = GenuineCheckState::ProcessingChallenge {
                            firmware_digest,
                            challenge,
                            attested: None,
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
        GenuineCheckState::ProcessingChallenge {
            challenge,
            firmware_digest,
            attested,
        } => {
            match port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(msg))) => match msg.body.decode() {
                    Ok(DeviceSendBody::GenuineAttestation {
                        certificate,
                        ds_signature,
                    }) => {
                        match genuine_certificate::verify_attestation(
                            &certificate,
                            genuine_key,
                            msg.from,
                            &ds_signature,
                        ) {
                            Ok(body) => *attested = Some(body),
                            Err(e) => {
                                return GenuineCheckPollResult::Failed(
                                    Some(certificate.unverified_raw_serial()),
                                    format!("Device failed genuine check: {e}"),
                                );
                            }
                        }
                    }
                    Ok(DeviceSendBody::GenuineIdentityProof { signature }) => {
                        let Some(body) = attested.take() else {
                            return GenuineCheckPollResult::Failed(
                                None,
                                "identity proof arrived before the attestation".to_string(),
                            );
                        };
                        if let Err(e) =
                            genuine_certificate::verify_identity(msg.from, *challenge, &signature)
                        {
                            return GenuineCheckPollResult::Failed(
                                Some(body.raw_serial()),
                                format!("Device failed genuine check: {e}"),
                            );
                        }
                        *state = GenuineCheckState::Complete {
                            firmware_digest: *firmware_digest,
                            serial: body.raw_serial(),
                        };
                    }
                    _ => {}
                },
                Ok(_) => {}
                Err(e) => {
                    return GenuineCheckPollResult::Failed(None, format!("Read error: {e}"));
                }
            }
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
) -> Result<(frostsnap_core::DeviceId, Sha256Digest), Box<dyn std::error::Error>> {
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

fn queue_genuine_requests(
    port: &mut FramedSerialPort<Downstream>,
    device_id: frostsnap_core::DeviceId,
    challenge: GenuineChallenge,
) {
    port.queue_send(
        CoordinatorSendMessage::to(device_id, CoordinatorSendBody::RequestGenuineAttestation)
            .into(),
    );
    port.queue_send(
        CoordinatorSendMessage::to(
            device_id,
            CoordinatorSendBody::GenuineIdentityChallenge(Box::new(challenge)),
        )
        .into(),
    );
}

fn wait_for_genuine_responses(
    port: &mut FramedSerialPort<Downstream>,
) -> Result<(Attestation, Signature), Box<dyn std::error::Error>> {
    let mut attestation = None;
    loop {
        port.poll_send()?;
        match port.try_read_message() {
            Ok(Some(ReceiveSerial::Message(msg))) => match msg.body.decode() {
                Ok(DeviceSendBody::GenuineAttestation {
                    certificate,
                    ds_signature,
                }) => {
                    attestation = Some(Attestation {
                        certificate: *certificate,
                        ds_signature,
                    });
                }
                Ok(DeviceSendBody::GenuineIdentityProof { signature }) => {
                    let attestation =
                        attestation.ok_or("identity proof arrived before the attestation")?;
                    return Ok((attestation, signature));
                }
                _ => {}
            },
            Ok(_) => {}
            Err(e) => return Err(format!("Read error: {e}").into()),
        }
    }
}

fn find_factory_key<'a>(
    certificate: &frostsnap_comms::genuine_certificate::Certificate,
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

    println!("Sending genuine check requests...");
    let challenge = GenuineChallenge::random(&mut rand::thread_rng());
    port.queue_send(CoordinatorSendMessage::to(device_id, CoordinatorSendBody::AnnounceAck).into());
    queue_genuine_requests(&mut port, device_id, challenge);

    println!("Waiting for genuine check responses...");
    let (attestation, identity_signature) = wait_for_genuine_responses(&mut port)?;

    let (env_name, factory_key) = find_factory_key(&attestation.certificate, known_keys)?;
    let certificate_body = genuine_certificate::verify_attestation(
        &attestation.certificate,
        factory_key,
        device_id,
        &attestation.ds_signature,
    )?;
    genuine_certificate::verify_identity(device_id, challenge, &identity_signature)?;

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
