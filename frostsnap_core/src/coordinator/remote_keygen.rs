use super::{
    keys::{BeginKeygen, KeyMutation},
    BroadcastPayload, CompleteKey, CoordinatorSend, CoordinatorToUserKeyGenMessage,
    CoordinatorToUserMessage, KeyPurpose, Mutation,
};
use crate::{
    message::{
        keygen::{self, Keygen},
        KeyGenAck, KeyGenResponse,
    },
    symmetric_encryption::Ciphertext,
    tweak::Xpub,
    AccessStructureId, AccessStructureKind, AccessStructureRef, ActionError, DeviceId, Error,
    KeygenId, MasterAppkey, SessionHash,
};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use schnorr_fun::frost::ShareIndex;
use schnorr_fun::{
    frost::{
        chilldkg::certpedpop::{self, vrf_cert},
        SharedKey,
    },
    fun::{prelude::*, KeyPair},
};
use sha2::Sha256;

const MSG_KIND: &str = "remote_keygen";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub(super) active_keygens: BTreeMap<KeygenId, RemoteKeygenState>,
}

impl State {
    pub fn clear_tmp_data(&mut self) {
        self.active_keygens.clear();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RemoteKeygenState {
    pub device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
    pub local_devices: BTreeSet<DeviceId>,
    pub pending_key_name: String,
    pub purpose: KeyPurpose,
    pub keygen_id: KeygenId,
    buffered: Vec<RemoteKeygenMessage>,
    pub phase: RemoteKeygenPhase,
}

impl RemoteKeygenState {
    pub fn new(
        device_to_share_index: BTreeMap<DeviceId, core::num::NonZeroU32>,
        local_devices: BTreeSet<DeviceId>,
        pending_key_name: String,
        purpose: KeyPurpose,
        keygen_id: KeygenId,
        phase: RemoteKeygenPhase,
    ) -> Self {
        Self {
            device_to_share_index,
            local_devices,
            pending_key_name,
            purpose,
            keygen_id,
            buffered: vec![],
            phase,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum RemoteKeygenPhase {
    WaitingForInputs {
        input_aggregator: certpedpop::Coordinator,
        coordinator_ids: Vec<DeviceId>,
        contributer: Box<certpedpop::Contributor>,
        my_keypair: KeyPair,
    },
    WaitingForCertificates {
        certifier: certpedpop::Certifier<vrf_cert::VrfCertScheme<Sha256>>,
        coordinator_keypair: KeyPair,
    },
    WaitingForAcks {
        certified_keygen: certpedpop::CertifiedKeygen<vrf_cert::CertVrfProof>,
        acks: BTreeSet<DeviceId>,
    },
    NeedsFinalize {
        root_shared_key: SharedKey,
    },
    #[doc(hidden)]
    Transitioning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteKeygenMessage {
    pub from: DeviceId,
    pub payload: RemoteKeygenPayload,
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub enum RemoteKeygenPayload {
    Input(certpedpop::KeygenInput),
    Certification(vrf_cert::CertVrfProof),
    Ack(SessionHash),
}

fn err(reason: impl ToString) -> Error {
    Error::coordinator_invalid_message(MSG_KIND, reason)
}

/// Resolve a DeviceId to its input_gen_index.
/// Returns (index, is_device).
fn resolve_input_index(
    coordinator_ids: &[DeviceId],
    device_to_share_index: &BTreeMap<DeviceId, core::num::NonZeroU32>,
    from: DeviceId,
) -> Result<(u32, bool), Error> {
    if let Some(coord_index) = coordinator_ids.iter().position(|id| *id == from) {
        return Ok((coord_index as u32, false));
    }
    if let Some(share_index) = device_to_share_index.get(&from) {
        let n_coordinators = coordinator_ids.len() as u32;
        return Ok((u32::from(*share_index) - 1 + n_coordinators, true));
    }
    Err(err("sender not part of this keygen"))
}

fn process_remote_keygen_msg(
    state: &mut RemoteKeygenState,
    keygen_id: KeygenId,
    msg: RemoteKeygenMessage,
    keygen_fingerprint: schnorr_fun::frost::Fingerprint,
) -> Result<Vec<CoordinatorSend>, Error> {
    let RemoteKeygenMessage { from, payload } = msg;

    match payload {
        RemoteKeygenPayload::Input(input) => {
            let RemoteKeygenPhase::WaitingForInputs {
                ref mut input_aggregator,
                ref coordinator_ids,
                ..
            } = state.phase
            else {
                return Ok(vec![]);
            };

            let (input_gen_index, is_device) =
                resolve_input_index(coordinator_ids, &state.device_to_share_index, from)?;

            match input_aggregator.add_input(
                &schnorr_fun::new_with_deterministic_nonces::<Sha256>(),
                input_gen_index,
                input,
            ) {
                Ok(()) => {}
                Err("we already have input from this party") => {
                    return Ok(vec![]);
                }
                Err(e) => return Err(err(format!("failed to add input: {e}"))),
            }

            let mut outgoing = vec![];
            if is_device {
                outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                    keygen_id,
                    inner: CoordinatorToUserKeyGenMessage::ReceivedShares { from },
                }));
            }

            if input_aggregator.is_finished() {
                let RemoteKeygenPhase::WaitingForInputs {
                    input_aggregator,
                    coordinator_ids,
                    contributer,
                    my_keypair,
                } = core::mem::replace(&mut state.phase, RemoteKeygenPhase::Transitioning)
                else {
                    unreachable!()
                };

                let mut agg_input = input_aggregator.finish().unwrap();
                agg_input.grind_fingerprint::<Sha256>(keygen_fingerprint);

                let cert_scheme = vrf_cert::VrfCertScheme::<Sha256>::new(
                    crate::message::keygen::VRF_CERT_SCHEME_ID,
                );

                let my_pk = my_keypair.public_key();
                let my_device_id = DeviceId(my_pk.to_bytes());
                let sig = contributer
                    .verify_agg_input(&cert_scheme, &agg_input, &my_keypair)
                    .expect("will be able to certify agg_input we created");

                let coordinator_public_keys: Vec<Point> =
                    coordinator_ids.iter().map(|id| id.pubkey()).collect();

                let mut certifier = certpedpop::Certifier::new(
                    cert_scheme,
                    agg_input.clone(),
                    &coordinator_public_keys,
                );

                certifier
                    .receive_certificate(my_pk, sig.clone())
                    .expect("will be able to verify our own certificate");

                outgoing.push(CoordinatorSend::ToDevice {
                    destinations: state.local_devices.clone(),
                    message: Keygen::CertifyPlease {
                        keygen_id,
                        agg_input,
                    }
                    .into(),
                });

                outgoing.push(CoordinatorSend::Broadcast {
                    channel: state.keygen_id,
                    from: my_device_id,
                    payload: BroadcastPayload::RemoteKeygen(RemoteKeygenPayload::Certification(
                        sig,
                    )),
                });

                state.phase = RemoteKeygenPhase::WaitingForCertificates {
                    certifier,
                    coordinator_keypair: my_keypair,
                };
            }

            Ok(outgoing)
        }
        RemoteKeygenPayload::Certification(vrf_cert) => {
            let RemoteKeygenPhase::WaitingForCertificates {
                ref mut certifier, ..
            } = state.phase
            else {
                state.buffered.push(RemoteKeygenMessage {
                    from,
                    payload: RemoteKeygenPayload::Certification(vrf_cert),
                });
                return Ok(vec![]);
            };

            certifier
                .receive_certificate(from.pubkey(), vrf_cert)
                .map_err(|_| err("invalid VRF proof received"))?;

            let mut outgoing = vec![];

            if certifier.is_finished() {
                let RemoteKeygenPhase::WaitingForCertificates { certifier, .. } =
                    core::mem::replace(&mut state.phase, RemoteKeygenPhase::Transitioning)
                else {
                    unreachable!()
                };

                let certified_keygen = certifier.finish().expect("just checked is_finished");
                let session_hash = SessionHash::from_certified_keygen(&certified_keygen);

                let certificate = certified_keygen
                    .certificate()
                    .iter()
                    .map(|(pk, c)| (*pk, c.clone()))
                    .collect();

                outgoing.extend([
                    CoordinatorSend::ToDevice {
                        destinations: state.local_devices.clone(),
                        message: Keygen::Check {
                            keygen_id,
                            certificate,
                        }
                        .into(),
                    },
                    CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                        keygen_id,
                        inner: CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash },
                    }),
                ]);

                if state.local_devices.is_empty() {
                    let root_shared_key = certified_keygen
                        .agg_input()
                        .shared_key()
                        .non_zero()
                        .expect("can't be zero, we contributed to it");
                    state.phase = RemoteKeygenPhase::NeedsFinalize { root_shared_key };
                } else {
                    state.phase = RemoteKeygenPhase::WaitingForAcks {
                        certified_keygen,
                        acks: Default::default(),
                    };
                }
            }

            Ok(outgoing)
        }
        RemoteKeygenPayload::Ack(session_hash) => {
            let RemoteKeygenPhase::WaitingForAcks {
                ref certified_keygen,
                ref mut acks,
            } = state.phase
            else {
                state.buffered.push(RemoteKeygenMessage {
                    from,
                    payload: RemoteKeygenPayload::Ack(session_hash),
                });
                return Ok(vec![]);
            };

            let expected_hash = SessionHash::from_certified_keygen(certified_keygen);
            if session_hash != expected_hash {
                return Err(err("device acked wrong session hash"));
            }

            if !state.device_to_share_index.contains_key(&from) {
                return Err(err("ack from unknown device"));
            }

            if !acks.insert(from) {
                return Ok(vec![]);
            }
            let all_acks_received = acks.len() == state.device_to_share_index.len();

            let outgoing = vec![CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                keygen_id,
                inner: CoordinatorToUserKeyGenMessage::KeyGenAck {
                    from,
                    all_acks_received,
                },
            })];

            if all_acks_received {
                let root_shared_key = certified_keygen
                    .agg_input()
                    .shared_key()
                    .non_zero()
                    .expect("can't be zero, we contributed to it");

                state.phase = RemoteKeygenPhase::NeedsFinalize { root_shared_key };
            }

            Ok(outgoing)
        }
    }
}

impl super::FrostCoordinator {
    pub fn begin_remote_keygen(
        &mut self,
        begin_keygen: BeginKeygen,
        coordinator_ids: &[DeviceId],
        local_devices: &BTreeSet<DeviceId>,
        my_keypair: KeyPair,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<impl IntoIterator<Item = CoordinatorSend>, ActionError> {
        let keygen_id = begin_keygen.keygen_id;

        if self.pending_keygens.contains_key(&keygen_id)
            || self.remote_keygen.active_keygens.contains_key(&keygen_id)
        {
            return Err(ActionError::StateInconsistent(
                "keygen with that id already in progress".into(),
            ));
        }

        let BeginKeygen {
            device_to_share_index,
            threshold,
            key_name,
            purpose,
            keygen_id,
            devices_in_order,
        } = begin_keygen;

        let my_device_id = DeviceId(my_keypair.public_key().to_bytes());
        let my_coordinator_index = coordinator_ids
            .iter()
            .position(|id| *id == my_device_id)
            .expect("my_keypair must correspond to an id in coordinator_ids")
            as u32;

        let n_devices = device_to_share_index.len();
        let n_coordinators = coordinator_ids.len();

        if n_devices < threshold as usize {
            panic!(
                "caller needs to ensure that threshold <= devices.len(). Tried {threshold}-of-{n_devices}",
            );
        }

        let share_receivers_enckeys = device_to_share_index
            .iter()
            .map(|(device, share_index)| (ShareIndex::from(*share_index), device.pubkey()))
            .collect::<BTreeMap<_, _>>();
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<Sha256>();
        let mut input_aggregator = certpedpop::Coordinator::new(
            threshold.into(),
            (n_devices + n_coordinators) as u32,
            &share_receivers_enckeys,
        );
        let (contributer, input) = certpedpop::Contributor::gen_keygen_input(
            &schnorr,
            threshold.into(),
            &share_receivers_enckeys,
            my_coordinator_index,
            rng,
        );
        input_aggregator
            .add_input(&schnorr, my_coordinator_index, input.clone())
            .expect("we just generated the input");

        let coordinator_public_keys: Vec<Point> =
            coordinator_ids.iter().map(|id| id.pubkey()).collect();

        self.remote_keygen.active_keygens.insert(
            keygen_id,
            RemoteKeygenState::new(
                device_to_share_index.clone(),
                local_devices.clone(),
                key_name.clone(),
                purpose,
                keygen_id,
                RemoteKeygenPhase::WaitingForInputs {
                    input_aggregator,
                    coordinator_ids: coordinator_ids.to_vec(),
                    contributer: Box::new(contributer),
                    my_keypair,
                },
            ),
        );

        let begin_message = keygen::Begin {
            devices: devices_in_order,
            threshold,
            key_name,
            purpose,
            keygen_id,
            coordinator_public_keys,
        };

        Ok(vec![
            CoordinatorSend::ToDevice {
                destinations: local_devices.clone(),
                message: begin_message.into(),
            },
            CoordinatorSend::Broadcast {
                channel: keygen_id,
                from: my_device_id,
                payload: BroadcastPayload::RemoteKeygen(RemoteKeygenPayload::Input(input)),
            },
        ])
    }

    pub(super) fn is_remote_keygen_active(&self, keygen_id: KeygenId) -> bool {
        self.remote_keygen.active_keygens.contains_key(&keygen_id)
    }

    pub(super) fn receive_device_keygen_response(
        &mut self,
        from: DeviceId,
        response: KeyGenResponse,
    ) -> crate::MessageResult<Vec<CoordinatorSend>> {
        let keygen_id = response.keygen_id;
        let payload = RemoteKeygenPayload::Input(*response.input);
        let mut outgoing = self.apply_keygen_message(
            keygen_id,
            RemoteKeygenMessage {
                from,
                payload: payload.clone(),
            },
        )?;
        outgoing.push(CoordinatorSend::Broadcast {
            channel: keygen_id,
            from,
            payload: BroadcastPayload::RemoteKeygen(payload),
        });
        Ok(outgoing)
    }

    pub(super) fn receive_device_keygen_certify(
        &mut self,
        from: DeviceId,
        keygen_id: KeygenId,
        vrf_cert: vrf_cert::CertVrfProof,
    ) -> crate::MessageResult<Vec<CoordinatorSend>> {
        let payload = RemoteKeygenPayload::Certification(vrf_cert);
        let mut outgoing = self.apply_keygen_message(
            keygen_id,
            RemoteKeygenMessage {
                from,
                payload: payload.clone(),
            },
        )?;
        outgoing.push(CoordinatorSend::Broadcast {
            channel: keygen_id,
            from,
            payload: BroadcastPayload::RemoteKeygen(payload),
        });
        Ok(outgoing)
    }

    pub(super) fn receive_device_keygen_ack(
        &mut self,
        from: DeviceId,
        ack: KeyGenAck,
    ) -> crate::MessageResult<Vec<CoordinatorSend>> {
        let keygen_id = ack.keygen_id;
        let payload = RemoteKeygenPayload::Ack(ack.ack_session_hash);
        let mut outgoing = self.apply_keygen_message(
            keygen_id,
            RemoteKeygenMessage {
                from,
                payload: payload.clone(),
            },
        )?;
        outgoing.push(CoordinatorSend::Broadcast {
            channel: keygen_id,
            from,
            payload: BroadcastPayload::RemoteKeygen(payload),
        });
        Ok(outgoing)
    }

    pub fn cancel_remote_keygen(&mut self, keygen_id: KeygenId) {
        let _ = self.remote_keygen.active_keygens.remove(&keygen_id);
    }

    pub fn apply_keygen_message(
        &mut self,
        keygen_id: KeygenId,
        msg: RemoteKeygenMessage,
    ) -> crate::MessageResult<Vec<CoordinatorSend>> {
        let Some(state) = self.remote_keygen.active_keygens.get_mut(&keygen_id) else {
            return Ok(vec![]);
        };

        let mut phase_tag = core::mem::discriminant(&state.phase);
        let mut outgoing =
            process_remote_keygen_msg(state, keygen_id, msg, self.keygen_fingerprint)?;

        while core::mem::discriminant(&state.phase) != phase_tag && !state.buffered.is_empty() {
            phase_tag = core::mem::discriminant(&state.phase);
            let buffered = core::mem::take(&mut state.buffered);
            for msg in buffered {
                let tag = core::mem::discriminant(&state.phase);
                let extra =
                    process_remote_keygen_msg(state, keygen_id, msg, self.keygen_fingerprint)?;
                outgoing.extend(extra);
                if core::mem::discriminant(&state.phase) != tag {
                    break;
                }
            }
        }

        Ok(outgoing)
    }

    pub fn finalize_remote_keygen(
        &mut self,
        keygen_id: KeygenId,
        encryption_key: crate::SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<super::SendFinalizeKeygen, ActionError> {
        let state = self
            .remote_keygen
            .active_keygens
            .remove(&keygen_id)
            .ok_or_else(|| ActionError::StateInconsistent("no such remote keygen".into()))?;
        let RemoteKeygenPhase::NeedsFinalize { root_shared_key } = state.phase else {
            self.remote_keygen.active_keygens.insert(keygen_id, state);
            return Err(ActionError::StateInconsistent(
                "remote keygen not ready to finalize".into(),
            ));
        };

        // Build the access-structure metadata. Same shape `mutate_new_key`
        // produces for local keygen; reproduced here so the remote-keygen
        // path stays self-contained and so we can constrain the per-device
        // share mutations to *our* devices.
        let rootkey = root_shared_key.public_key();
        let xpub_root = Xpub::from_rootkey(root_shared_key);
        let app_shared_key = xpub_root.rootkey_to_master_appkey();
        let access_structure_id =
            AccessStructureId::from_app_poly(app_shared_key.key.point_polynomial());
        let encrypted_rootkey = Ciphertext::encrypt(encryption_key, &rootkey, rng);
        let master_appkey = MasterAppkey::from_xpub_unchecked(&app_shared_key);
        let key_id = master_appkey.key_id();
        let access_structure_ref = AccessStructureRef {
            key_id,
            access_structure_id,
        };

        if self.get_frost_key(key_id).is_none() {
            self.mutate(Mutation::Keygen(KeyMutation::NewKey {
                key_name: state.pending_key_name,
                purpose: state.purpose,
                complete_key: CompleteKey {
                    master_appkey,
                    encrypted_rootkey,
                    access_structures: Default::default(),
                },
            }));
        }

        self.mutate(Mutation::Keygen(KeyMutation::NewAccessStructure {
            shared_key: app_shared_key,
            kind: AccessStructureKind::Master,
        }));

        // CRITICAL: only record share-index mappings for *our local devices*.
        // The other coordinators' devices participated in the keygen but we
        // don't hold their encrypted shares — claiming we do via NewShare
        // mutations would lie to every later code path that walks the
        // access structure (signing, recovery, backup).
        for device_id in &state.local_devices {
            let share_index = state.device_to_share_index[device_id];
            self.mutate(Mutation::Keygen(KeyMutation::NewShare {
                access_structure_ref,
                device_id: *device_id,
                share_index: ShareIndex::from(share_index),
            }));
        }

        Ok(super::SendFinalizeKeygen {
            devices: state.local_devices.iter().copied().collect(),
            access_structure_ref,
            keygen_id,
        })
    }
}
