use bdk_chain::bitcoin::util::taproot::TapSighashHash;
use frostsnap_comms::{DeviceReceiveBody, DeviceReceiveMessage};
use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToUserMessage, DeviceToCoordinatorBody, DeviceToCoordindatorMessage,
};
use frostsnap_core::schnorr_fun;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tracing::{event, Level};

use crate::{db::Db, nostr, ports::Ports};

use anyhow::anyhow;

pub struct Signer<'a, 'b> {
    // key: CoordinatorFrostKey,
    // still_need_to_sign: BTreeSet<DeviceId>,
    coordinator: frostsnap_core::FrostCoordinator,
    ports: &'a mut Ports,
    db: &'b mut Db,
}

impl<'a, 'b> Signer<'a, 'b> {
    pub fn new(
        db: &'b mut Db,
        ports: &'a mut Ports,
        coordinator: frostsnap_core::FrostCoordinator,
    ) -> Self {
        Self {
            coordinator,
            ports,
            db,
        }
    }

    pub fn sign_plain_message(
        &mut self,
        message: frostsnap_ext::sign_messages::RequestSignMessage,
    ) -> anyhow::Result<Vec<schnorr_fun::Signature>> {
        let finished_signatures = self.run_signing_process(message, false)?;
        Ok(finished_signatures)
    }

    pub fn sign_nostr(&mut self, message: String) -> anyhow::Result<nostr::Event> {
        let key = self
            .coordinator
            .key()
            .ok_or(anyhow!("Incorrect state to start signing"))?;
        let public_key = key.frost_key().clone().into_xonly_key().public_key();
        let time_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Failed to retrieve system time")
            .as_secs();

        let event = nostr::UnsignedEvent::new(public_key, 1, vec![], message, time_now as i64);

        let finished_signature = self.run_signing_process(
            frostsnap_ext::sign_messages::RequestSignMessage::Nostr(event.clone()),
            false,
        )?;
        let finished_signature = finished_signature[0].clone();
        let signed_event = event.add_signature(finished_signature);

        Ok(signed_event)
    }

    fn run_signing_process(
        &mut self,
        message: frostsnap_ext::sign_messages::RequestSignMessage,
        tap_tweak: bool,
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

        let (init_sends, signature_request) =
            self.coordinator
                .start_sign(message, tap_tweak, still_need_to_sign.clone())?;

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

            let (newly_registered, new_messages) = self.ports.poll_devices();
            let asking_to_sign = newly_registered
                .intersection(&still_need_to_sign)
                .cloned()
                .collect::<BTreeSet<_>>();

            self.ports.queue_in_port_outbox(vec![DeviceReceiveMessage {
                target_destinations: asking_to_sign.clone(),
                message_body: DeviceReceiveBody::Core(signature_request.clone()),
            }]);
            self.ports.send_to_devices()?;

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
        };
        Ok(finished_signatures.clone())
    }

    pub fn sign_tap_tweaked(
        &mut self,
        tx_sighashes: Vec<TapSighashHash>,
    ) -> anyhow::Result<Vec<schnorr_fun::Signature>> {
        let message_sighashes = tx_sighashes
            .into_iter()
            .map(|sighash| sighash.to_vec())
            .collect();
        let messages =
            frostsnap_ext::sign_messages::RequestSignMessage::SigHashes(message_sighashes);
        let finished_signatures = self.run_signing_process(messages, true)?;
        Ok(finished_signatures)
    }
}
