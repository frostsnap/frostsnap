use frostsnap_comms::{DeviceReceiveBody, DeviceReceiveMessage};
use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToUserMessage, DeviceToCoordinatorBody,
    DeviceToCoordindatorMessage, SignTask,
};
use frostsnap_core::{schnorr_fun, CoordinatorFrostKey};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tracing::{event, Level};

use crate::db::Db;
use crate::serial::DesktopSerial;

use anyhow::anyhow;

pub struct Signer<'a, 'b> {
    // key: CoordinatorFrostKey,
    // still_need_to_sign: BTreeSet<DeviceId>,
    coordinator: frostsnap_core::FrostCoordinator,
    ports: &'a mut frostsnap_coordinator::UsbSerialManager<DesktopSerial>,
    db: &'b mut Db,
}

impl<'a, 'b> Signer<'a, 'b> {
    pub fn new(
        db: &'b mut Db,
        ports: &'a mut frostsnap_coordinator::UsbSerialManager<DesktopSerial>,
        coordinator: frostsnap_core::FrostCoordinator,
    ) -> Self {
        Self {
            coordinator,
            ports,
            db,
        }
    }

    pub fn sign_message_request(
        &mut self,
        message: SignTask,
    ) -> anyhow::Result<Vec<schnorr_fun::Signature>> {
        let finished_signatures = self.run_signing_process(message)?;
        Ok(finished_signatures)
    }

    pub fn coordinator_frost_key(&self) -> anyhow::Result<&CoordinatorFrostKey> {
        Ok(self
            .coordinator
            .key()
            .ok_or(anyhow!("Incorrect state to start signing"))?)
    }

    fn run_signing_process(
        &mut self,
        message: SignTask,
    ) -> anyhow::Result<Vec<frostsnap_core::schnorr_fun::Signature>> {
        let key = self
            .coordinator
            .key()
            .ok_or(anyhow!("Incorrect state to start signing"))?;

        let key_signers: BTreeMap<_, _> = key
            .devices()
            .map(|device_id| {
                (
                    device_id,
                    self.ports
                        .device_labels()
                        .get(&device_id)
                        .expect("device in key must be known to coordinator")
                        .to_string(),
                )
            })
            .collect();

        let chosen_signers = if key_signers.len() != key.frost_key().threshold() {
            eprintln!("Choose {} devices to sign:", key.frost_key().threshold());
            crate::choose_devices(&key_signers, key.frost_key().threshold())
        } else {
            key_signers.keys().cloned().collect()
        };

        let mut still_need_to_sign = chosen_signers.clone();

        eprintln!(
            "Plug signers:\n{}",
            still_need_to_sign
                .iter()
                .map(|device_id| self
                    .ports
                    .device_labels()
                    .get(device_id)
                    .expect("we must have labelled this signer")
                    .clone())
                .collect::<Vec<_>>()
                .join("\n")
        );

        let (init_sends, signature_request) = self
            .coordinator
            .start_sign(message, still_need_to_sign.clone())?;

        let mut outbox = VecDeque::from_iter(init_sends);
        let mut signatures = None;
        let finished_signatures = loop {
            signatures = signatures.or_else(|| {
                outbox.iter().find_map(|message| match message {
                    CoordinatorSend::ToUser(CoordinatorToUserMessage::Signed { signatures }) => {
                        Some(signatures.clone())
                    }
                    _ => None,
                })
            });
            crate::process_outbox(self.db, &mut self.coordinator, &mut outbox, self.ports)?;

            if let Some(finished_signatures) = &signatures {
                if outbox.is_empty() {
                    break finished_signatures;
                }
            }

            let mut newly_registered = BTreeSet::new();
            let mut start = std::time::Instant::now();

            // this loop is here to wait a bit before sending out signing requests to devices
            // because often a big bunch of devices will register at similar times if they are daisy
            // chained together.
            loop {
                let (just_now_registered_devices, new_messages) = self.ports.poll_ports();

                for incoming in new_messages {
                    match self.coordinator.recv_device_message(incoming.clone()) {
                        Ok(outgoing) => {
                            if let DeviceToCoordindatorMessage {
                                from,
                                body: DeviceToCoordinatorBody::SignatureShare { .. },
                            } = incoming
                            {
                                event!(Level::INFO, "{} signed successfully", incoming.from);
                                still_need_to_sign.remove(&from);
                            }
                            outbox.extend(outgoing);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                "Failed to process message from {}: {}",
                                incoming.from,
                                e
                            );
                            continue;
                        }
                    };
                }

                if just_now_registered_devices.len() > 0 {
                    start = std::time::Instant::now();
                    newly_registered.extend(just_now_registered_devices);
                } else if start.elapsed().as_millis() > 3_000 {
                    break;
                }
            }

            let asking_to_sign = newly_registered
                .intersection(&still_need_to_sign)
                .cloned()
                .collect::<BTreeSet<_>>();
            let message = DeviceReceiveMessage {
                target_destinations: asking_to_sign.clone(),
                message_body: DeviceReceiveBody::Core(signature_request.clone()),
            };

            dbg!(&message);
            self.ports.queue_in_port_outbox(message);
        };

        Ok(finished_signatures.clone())
    }
}
