use frostsnap_coordinator::{DeviceChange, SigningDispatcher};
use frostsnap_core::message::{CoordinatorSend, CoordinatorToUserMessage, SignTask};
use frostsnap_core::schnorr_fun;
use frostsnap_core::schnorr_fun::frost::FrostKey;
use frostsnap_core::schnorr_fun::fun::marker::Normal;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;
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
        Ok(self
            .coordinator
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
            eprintln!(
                "Choose {} devices to sign:",
                key_state.frost_key().threshold()
            );
            crate::choose_devices(&key_signers, key_state.frost_key().threshold())
        } else {
            key_signers.keys().cloned().collect()
        };

        let mut sign_request_sends = self
            .coordinator
            .start_sign(message, chosen_signers.clone())?;

        let mut dispatcher = SigningDispatcher::from_filter_out_start_sign(&mut sign_request_sends);

        for device in self.ports.registered_devices() {
            dispatcher.connected(*device);
        }

        eprintln!(
            "Plug signers:\n{}",
            chosen_signers
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

        let mut outbox = VecDeque::from_iter(sign_request_sends);
        let finished_signatures = loop {
            for message in &outbox {
                if let CoordinatorSend::ToUser(CoordinatorToUserMessage::Signing(signing_message)) =
                    message
                {
                    dispatcher.process_to_user_message(signing_message.clone());
                }
            }
            crate::process_outbox(self.db, &mut self.coordinator, &mut outbox, self.ports)?;

            if let Some(finished_signatures) = dispatcher.finished_signatures.take() {
                if outbox.is_empty() {
                    break finished_signatures;
                }
            }

            let mut waiting_start = std::time::Instant::now();

            // this loop is here to wait a bit before sending out signing requests to devices
            // because often a big bunch of devices will register at similar times if they are daisy
            // chained together.
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let port_changes = self.ports.poll_ports();

                for (from, incoming_message) in port_changes.new_messages {
                    match self.coordinator.recv_device_message(from, incoming_message) {
                        Ok(outgoing) => {
                            outbox.extend(outgoing);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                "Failed to process message from {}: {}",
                                from,
                                e
                            );
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
                            dispatcher.disconnected(*id);
                        }
                        DeviceChange::Registered { id, .. } => {
                            dispatcher.connected(*id);
                        }
                        DeviceChange::Connected { .. } | DeviceChange::NewUnknownDevice { .. } => { /* do nothing until it's registered */
                        }
                    }
                }

                if !port_changes.device_changes.is_empty() {
                    waiting_start = std::time::Instant::now();
                } else if waiting_start.elapsed().as_millis() > 2_000 {
                    break;
                }
            }

            if let Some(send_message) = dispatcher.resend_sign_request() {
                self.ports.queue_in_port_outbox(send_message);
            }
        };

        Ok(finished_signatures
            .iter()
            .map(|sig| sig.into_decoded().unwrap())
            .collect())
    }
}
