use frostsnap_comms::genuine_certificate::{CaseColor, CertificateBody, GenuineError};
use frostsnap_comms::{
    genuine_check as protocol, CoordinatorSendBody, CoordinatorSendMessage, DeviceSendBody,
    Downstream, ReceiveSerial, Sha256Digest, MAGIC_BYTES_PERIOD,
};
use frostsnap_coordinator::genuine_check::{GenuineCheck, GenuineStatus};
use frostsnap_coordinator::{DesktopSerial, FirmwareVersion, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun::fun::{marker::EvenY, Point};
use frostsnap_core::DeviceId;
use rand::RngCore;
use std::collections::VecDeque;
use std::time::Instant;

use crate::{USB_PID, USB_VID};

pub enum GenuineCheckState {
    WaitingForMagic {
        last_wrote: Option<Instant>,
    },
    WaitingForAnnounce,
    Checking {
        firmware_digest: Sha256Digest,
        check: Box<GenuineCheck>,
        /// What the device's certificate claims, once it has shown one.
        serial: Option<String>,
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

                        let mut check = GenuineCheck::new(genuine_key);
                        let Some(request) = check.connected(
                            msg.from,
                            FirmwareVersion::new(firmware_digest),
                            &mut rand::thread_rng(),
                        ) else {
                            return GenuineCheckPollResult::Failed(
                                None,
                                "Device firmware does not support the genuine check".to_string(),
                            );
                        };
                        port.queue_send(
                            CoordinatorSendMessage::to(msg.from, request.into()).into(),
                        );
                        *state = GenuineCheckState::Checking {
                            firmware_digest,
                            check: Box::new(check),
                            serial: None,
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
        GenuineCheckState::Checking {
            firmware_digest,
            check,
            serial,
        } => {
            match port.try_read_message() {
                Ok(Some(ReceiveSerial::Message(msg))) => {
                    if let Ok(DeviceSendBody::GenuineCheck(message)) = msg.body.decode() {
                        if check.status(msg.from).is_none() {
                            return GenuineCheckPollResult::Continue;
                        }
                        if let protocol::DeviceMessage::FactoryAttestation { attested, .. } =
                            &message
                        {
                            *serial = Some(attested.certificate().unverified_raw_serial());
                        }
                        match check.recv(
                            msg.from,
                            message,
                            &mut VecDeque::new(),
                            &mut rand::thread_rng(),
                        ) {
                            Ok(Some(request)) => port.queue_send(
                                CoordinatorSendMessage::to(msg.from, request.into()).into(),
                            ),
                            Ok(None) => {
                                if let Some(GenuineStatus::Genuine { certificate }) =
                                    check.status(msg.from)
                                {
                                    *state = GenuineCheckState::Complete {
                                        firmware_digest: *firmware_digest,
                                        serial: certificate.raw_serial(),
                                    };
                                }
                            }
                            Err(e) => {
                                return GenuineCheckPollResult::Failed(
                                    serial.clone(),
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

fn wait_for_genuine_message(
    port: &mut FramedSerialPort<Downstream>,
    device_id: DeviceId,
) -> Result<protocol::DeviceMessage, Box<dyn std::error::Error>> {
    loop {
        port.poll_send()?;
        match port.try_read_message() {
            Ok(Some(ReceiveSerial::Message(msg))) => {
                if msg.from != device_id {
                    continue;
                }
                if let Ok(DeviceSendBody::GenuineCheck(message)) = msg.body.decode() {
                    return Ok(message);
                }
            }
            Ok(_) => {}
            Err(e) => return Err(format!("Read error: {e}").into()),
        }
    }
}

/// Runs a complete check under each known key in turn until one passes, reporting that key's
/// environment.
fn check_under_known_keys<'a>(
    port: &mut FramedSerialPort<Downstream>,
    known_keys: &[(&'a str, Point<EvenY>)],
    device_id: DeviceId,
    firmware: FirmwareVersion,
    rng: &mut impl RngCore,
) -> Result<(&'a str, Box<CertificateBody>), Box<dyn std::error::Error>> {
    for &(env, key) in known_keys {
        let mut check = GenuineCheck::new(key);
        let mut request = check
            .connected(device_id, firmware, rng)
            .ok_or("Device firmware does not support the genuine check")?;
        loop {
            port.queue_send(CoordinatorSendMessage::to(device_id, request.into()).into());
            let reply = wait_for_genuine_message(port, device_id)?;
            match check.recv(device_id, reply, &mut VecDeque::new(), rng) {
                Ok(Some(next)) => request = next,
                Ok(None) => match check.status(device_id) {
                    Some(GenuineStatus::Genuine { certificate }) => return Ok((env, certificate)),
                    _ => return Err("Device sent a genuine check reply it wasn't asked for".into()),
                },
                Err(GenuineError::UnknownFactoryKey) => break,
                Err(e) => return Err(format!("Device failed genuine check: {e}").into()),
            }
        }
    }
    Err("Certificate not signed by any known genuine key".into())
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

    println!("Running genuine check...");
    port.queue_send(CoordinatorSendMessage::to(device_id, CoordinatorSendBody::AnnounceAck).into());
    let (env_name, certificate) = check_under_known_keys(
        &mut port,
        known_keys,
        device_id,
        FirmwareVersion::new(firmware_digest),
        &mut rand::thread_rng(),
    )?;

    let CertificateBody::Frontier {
        case_color,
        revision,
        serial,
        timestamp,
        ..
    } = &*certificate;

    Ok(GenuineCheckResult {
        serial: serial.clone(),
        color: *case_color,
        revision: revision.clone(),
        timestamp: *timestamp,
        firmware_digest,
        env: env_name.to_string(),
    })
}
