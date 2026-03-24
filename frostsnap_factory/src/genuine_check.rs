use frostsnap_comms::genuine_certificate::{CaseColor, CertificateBody, CertificateVerifier};
use frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, DeviceSendBody, Downstream, ReceiveSerial,
    Sha256Digest, MAGIC_BYTES_PERIOD,
};
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use rand::RngCore;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::RsaPublicKey;
use sha2::Digest;
use std::time::Instant;

use crate::{USB_PID, USB_VID};

pub enum GenuineCheckState {
    WaitingForMagic {
        last_wrote: Option<Instant>,
    },
    WaitingForAnnounce,
    ProcessingChallenge {
        firmware_digest: Sha256Digest,
        challenge: Box<[u8; 32]>,
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

                        let mut challenge = [0u8; 32];
                        rand::thread_rng().fill_bytes(&mut challenge);

                        port.queue_send(
                            CoordinatorSendMessage::to(
                                msg.from,
                                CoordinatorSendBody::Challenge(Box::new(challenge)),
                            )
                            .into(),
                        );
                        *state = GenuineCheckState::ProcessingChallenge {
                            firmware_digest,
                            challenge: Box::new(challenge),
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
        } => {
            match port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(msg))) => {
                    if let Ok(DeviceSendBody::SignedChallenge {
                        signature,
                        certificate,
                    }) = msg.body.decode()
                    {
                        let certificate_body =
                            match CertificateVerifier::verify(&certificate, genuine_key) {
                                Some(body) => body,
                                None => {
                                    return GenuineCheckPollResult::Failed(
                                        Some(certificate.unverified_raw_serial()),
                                        "genuine check failed to verify!".to_string(),
                                    );
                                }
                            };
                        let serial = certificate_body.raw_serial();

                        match verify_challenge_signature(&certificate_body, challenge, &signature) {
                            Ok(_) => {
                                *state = GenuineCheckState::Complete {
                                    firmware_digest: *firmware_digest,
                                    serial: serial.to_string(),
                                };
                            }
                            Err(e) => {
                                return GenuineCheckPollResult::Failed(
                                    Some(serial.clone()),
                                    format!("Device failed genuine check: {e}"),
                                );
                            }
                        }
                    }
                }
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

type SignedChallengeResponse = (
    frostsnap_comms::genuine_certificate::Certificate,
    Box<[u8; 384]>,
);

fn wait_for_signed_challenge(
    port: &mut FramedSerialPort<Downstream>,
) -> Result<SignedChallengeResponse, Box<dyn std::error::Error>> {
    loop {
        port.poll_send()?;
        match port.try_read_message() {
            Ok(Some(ReceiveSerial::Message(msg))) => {
                if let Ok(DeviceSendBody::SignedChallenge {
                    signature,
                    certificate,
                }) = msg.body.decode()
                {
                    return Ok((*certificate, signature));
                }
            }
            Ok(_) => {}
            Err(e) => return Err(format!("Read error: {e}").into()),
        }
    }
}

fn try_verify_certificate<'a>(
    certificate: &frostsnap_comms::genuine_certificate::Certificate,
    known_keys: &[(&'a str, Point<EvenY>)],
) -> Result<(&'a str, CertificateBody), Box<dyn std::error::Error>> {
    for (env, key) in known_keys {
        if let Some(body) = CertificateVerifier::verify(certificate, *key) {
            return Ok((env, body));
        }
    }
    Err("Certificate not signed by any known genuine key".into())
}

pub fn verify_challenge_signature(
    certificate_body: &CertificateBody,
    challenge: &[u8; 32],
    signature: &[u8; 384],
) -> Result<(), Box<dyn std::error::Error>> {
    let ds_public_key = RsaPublicKey::from_pkcs1_der(certificate_body.ds_public_key())?;
    let padding = rsa::Pkcs1v15Sign::new::<sha2::Sha256>();
    let message_digest: [u8; 32] = sha2::Sha256::digest(challenge).into();
    ds_public_key
        .verify(padding, &message_digest, signature.as_ref())
        .map_err(|e| format!("Challenge signature verification failed: {e}"))?;
    Ok(())
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

    println!("Sending challenge...");
    let mut challenge = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut challenge);
    port.queue_send(CoordinatorSendMessage::to(device_id, CoordinatorSendBody::AnnounceAck).into());
    port.queue_send(
        CoordinatorSendMessage::to(
            device_id,
            CoordinatorSendBody::Challenge(Box::new(challenge)),
        )
        .into(),
    );

    println!("Waiting for signed challenge...");
    let (certificate, signature) = wait_for_signed_challenge(&mut port)?;

    let (env_name, certificate_body) = try_verify_certificate(&certificate, known_keys)?;
    verify_challenge_signature(&certificate_body, &challenge, &signature)?;

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
