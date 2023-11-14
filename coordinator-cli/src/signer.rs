use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_coordinator::DeviceChange;
use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToUserMessage, DeviceToCoordinatorMessage, SignTask,
};
use frostsnap_core::schnorr_fun::frost::FrostKey;
use frostsnap_core::schnorr_fun::fun::marker::Normal;
use frostsnap_core::{schnorr_fun, DeviceId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tracing::{event, Level};

use crate::db::Db;

use anyhow::anyhow;

pub struct Signer<'a, 'b> {
    // key: CoordinatorFrostKey,
    // still_need_to_sign: BTreeSet<DeviceId>,
    coordinator: frostsnap_core::FrostCoordinator,
    ports: &'a mut frostsnap_coordinator::UsbSerialManager,
    db: &'b mut Db,
}

impl<'a, 'b> Signer<'a, 'b> {
    pub fn new(
        db: &'b mut Db,
        ports: &'a mut frostsnap_coordinator::UsbSerialManager,
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

    pub fn frost_key(&self) -> anyhow::Result<&FrostKey<Normal>> {
        Ok(self.coordinator
            .frost_key_state()
            .ok_or(anyhow!("Incorrect state to start signing"))?
            .frost_key())
    }

    fn run_signing_process(
        &mut self,
        message: SignTask,
    ) -> anyhow::Result<Vec<frostsnap_core::schnorr_fun::Signature>> {
        let key_state = self
            .coordinator
            .frost_key_state()
            .ok_or(anyhow!("Incorrect state to start signing"))?;

        let key_signers: BTreeMap<_, _> = key_state
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

        let chosen_signers = if key_signers.len() != key_state.frost_key().threshold() {
            eprintln!("Choose {} devices to sign:", key_state.frost_key().threshold());
            crate::choose_devices(&key_signers, key_state.frost_key().threshold())
        } else {
            key_signers.keys().cloned().collect()
        };

        let mut still_need_to_sign = chosen_signers.clone();
        let mut asking_to_sign: BTreeSet<DeviceId> = still_need_to_sign
            .intersection(self.ports.registered_devices())
            .cloned()
            .collect();

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

            let mut waiting_start = std::time::Instant::now();

            // this loop is here to wait a bit before sending out signing requests to devices
            // because often a big bunch of devices will register at similar times if they are daisy
            // chained together.
            loop {
                let port_changes = self.ports.poll_ports();
                for (from, incoming_message) in port_changes.new_messages {
                    let is_signature_share = matches!(
                        incoming_message,
                        DeviceToCoordinatorMessage::SignatureShare { .. }
                    );
                    match self.coordinator.recv_device_message(from, incoming_message) {
                        Ok(outgoing) => {
                            if is_signature_share {
                                event!(Level::INFO, "{} signed successfully", from);
                                still_need_to_sign.remove(&from);
                            }
                            outbox.extend(outgoing);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                "Failed to process message from {}: {}",
                                from,
                                e
                            );
                            continue;
                        }
                    };
                }

                for device_change in &port_changes.device_changes {
                    match device_change {
                        DeviceChange::NeedsName { .. } => {
                            /* don't name devices during keygen */
                            eprintln!("⚠ you've plugged in a device that hasn't been set up yet");
                        }
                        DeviceChange::Renamed {
                            id,
                            old_name,
                            new_name,
                        } => {
                            eprintln!(
                                "⚠ device {id} renamed to {new_name}. It's old name was {old_name}"
                            );
                        }
                        DeviceChange::Disconnected { id } => {
                            asking_to_sign.remove(id);
                        }
                        DeviceChange::Registered { id, .. } => {
                            if still_need_to_sign.contains(id) {
                                asking_to_sign.insert(*id);
                            }
                        }
                        DeviceChange::Added { .. } => { /* do nothing until it's registered */ }
                    }
                }

                if !port_changes.device_changes.is_empty() {
                    waiting_start = std::time::Instant::now();
                } else if waiting_start.elapsed().as_millis() > 2_000 {
                    break;
                }
            }

            self.ports.queue_in_port_outbox(CoordinatorSendMessage {
                target_destinations: frostsnap_comms::Destination::Particular(std::mem::take(&mut asking_to_sign)),
                message_body: CoordinatorSendBody::Core(signature_request.clone()),
            });
        };

        Ok(finished_signatures.clone())
    }
}
