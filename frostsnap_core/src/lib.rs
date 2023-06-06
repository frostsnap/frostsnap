#![no_std]

#[cfg(feature = "std")]
#[allow(unused)]
#[macro_use]
extern crate std;

pub mod encrypted_share;
pub mod message;
pub mod nostr;
pub mod xpub;

use bitcoin::XOnlyPublicKey;
use message::{CoordinatorToStorageMessage, DeviceToCoordinatorBody};
pub use schnorr_fun;

#[macro_use]
extern crate alloc;

use crate::{
    encrypted_share::EncryptedShare,
    message::{
        CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage, DeviceSend,
        DeviceToCoordindatorMessage, DeviceToUserMessage, KeyGenProvideShares, RequestSignMessage,
    },
};
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::String,
    string::ToString,
    vec::Vec,
};

use rand_chacha::ChaCha20Rng;
use rand_core::RngCore;
use schnorr_fun::{
    frost::{self, generate_scalar_poly, FrostKey, SignSession},
    fun::{derive_nonce_rng, marker::*, KeyPair, Point, Scalar, Tag},
    musig::{Nonce, NonceKeyPair},
    nonce, Message,
};
use sha2::digest::Digest;
use sha2::Sha256;

#[derive(Debug, Clone)]
pub struct FrostCoordinator {
    state: CoordinatorState,
}

pub const NONCE_BATCH_SIZE: usize = 10;

impl FrostCoordinator {
    pub fn new() -> Self {
        Self {
            state: CoordinatorState::Registration,
        }
    }

    pub fn from_stored_key(key: CoordinatorFrostKey) -> Self {
        Self {
            state: CoordinatorState::FrostKey {
                key,
                awaiting_user: false,
            },
        }
    }

    pub fn recv_device_message(
        &mut self,
        message: DeviceToCoordindatorMessage,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        match &mut self.state {
            CoordinatorState::Registration => {
                Err(Error::coordinator_message_kind(&self.state, &message))
            }
            CoordinatorState::KeyGen {
                shares: shares_provided,
            } => match message.body {
                DeviceToCoordinatorBody::KeyGenProvideShares(new_shares) => {
                    if let Some(existing) =
                        shares_provided.insert(message.from, Some(new_shares.clone()))
                    {
                        debug_assert!(existing.is_none() || existing == Some(new_shares));
                    }

                    let shares_provided = shares_provided
                        .clone()
                        .into_iter()
                        .map(|(device_id, shares)| Some((device_id, shares?)))
                        .collect::<Option<BTreeMap<_, _>>>();

                    match shares_provided {
                        Some(shares_provided) => {
                            let point_polys = shares_provided
                                .iter()
                                .map(|(device_id, share)| {
                                    (device_id.to_x_coord(), share.my_poly.clone())
                                })
                                .collect();
                            let proofs_of_possession = shares_provided
                                .iter()
                                .map(|(device_id, share)| {
                                    (device_id.to_x_coord(), share.proof_of_possession.clone())
                                })
                                .collect();
                            let frost = frost::new_without_nonce_generation::<Sha256>();
                            let keygen = frost.new_keygen(point_polys).unwrap();
                            // let keygen_id = frost.keygen_id(&keygen);
                            let pop_message = gen_pop_message(shares_provided.keys().cloned());

                            let frost_key = match frost.finish_keygen_coordinator(keygen, proofs_of_possession, Message::raw(&pop_message)) {
                                Ok(frost_key) => frost_key,
                                Err(_) => todo!("should notify user somehow that everything was fucked and we're canceling it"),
                            };

                            let xpub = frost_key.public_key().to_string();

                            let device_nonces = shares_provided
                                .iter()
                                .map(|(device_id, share)| {
                                    let device_nonces = DeviceNonces {
                                        counter: 0,
                                        nonces: share.nonces.iter().cloned().collect(),
                                    };
                                    (*device_id, device_nonces)
                                })
                                .collect();

                            let key = CoordinatorFrostKey {
                                frost_key,
                                device_nonces,
                            };
                            self.state = CoordinatorState::FrostKey {
                                key: key.clone(),
                                awaiting_user: true,
                            };
                            // TODO: check order
                            Ok(vec![
                                CoordinatorSend::ToStorage(
                                    CoordinatorToStorageMessage::UpdateState(key),
                                ),
                                CoordinatorSend::ToDevice(
                                    CoordinatorToDeviceMessage::FinishKeyGen {
                                        shares_provided: shares_provided.clone(),
                                    },
                                ),
                                CoordinatorSend::ToUser(CoordinatorToUserMessage::CheckKeyGen {
                                    xpub,
                                }),
                            ])
                        }
                        None =>
                        /* not finished yet  */
                        {
                            Ok(vec![])
                        }
                    }
                }
                _ => Err(Error::coordinator_message_kind(&self.state, &message)),
            },
            CoordinatorState::Signing {
                key,
                sessions,
                tap_tweak,
            } => match &message.body {
                DeviceToCoordinatorBody::SignatureShare {
                    signature_shares,
                    new_nonces,
                } => {
                    let n_signatures = sessions.len();
                    let frost = frost::new_without_nonce_generation::<Sha256>();

                    let nonce_for_device = key.device_nonces.get_mut(&message.from).ok_or(
                        Error::coordinator_invalid_message(&message, "Signer is unknown".into()),
                    )?;

                    if new_nonces.len() != n_signatures {
                        return Err(Error::coordinator_invalid_message(
                            &message,
                            format!(
                                "Signer did not replenish the correct number of nonces: {}",
                                n_signatures
                            ),
                        ));
                    }
                    let mut xonly_frost_key = key.frost_key.clone().into_xonly_key();

                    if *tap_tweak {
                        let tweak = bitcoin::util::taproot::TapTweakHash::from_key_and_tweak(
                            XOnlyPublicKey::from_slice(
                                &xonly_frost_key.public_key().to_xonly_bytes(),
                            )
                            .unwrap(),
                            None,
                        )
                        .to_scalar();
                        xonly_frost_key = xonly_frost_key
                            .tweak(
                                Scalar::<Public, Zero>::from_slice(&tweak.to_be_bytes()).unwrap(),
                            )
                            .unwrap();
                    }

                    if signature_shares.len() != n_signatures {
                        return Err(Error::coordinator_invalid_message(&message, format!("signer did not provide the right number of signature shares. Got {}, expected {}", signature_shares.len(), sessions.len())));
                    }

                    for (session_progress, signature_share) in
                        sessions.iter_mut().zip(signature_shares)
                    {
                        let session = &mut session_progress.sign_session;
                        if session
                            .participants()
                            .find(|x_coord| *x_coord == message.from.to_x_coord())
                            .is_none()
                        {
                            return Err(Error::coordinator_invalid_message(
                                &message,
                                "Signer was not a particpant for this session".into(),
                            ));
                        }

                        if frost.verify_signature_share(
                            &xonly_frost_key,
                            session,
                            message.from.to_x_coord(),
                            *signature_share,
                        ) {
                            session_progress
                                .signature_shares
                                .insert(message.from, *signature_share);
                        } else {
                            return Err(Error::coordinator_invalid_message(
                                &message,
                                format!(
                                    "Inavlid signature share under key {}",
                                    xonly_frost_key.public_key()
                                ),
                            ));
                        }
                    }

                    nonce_for_device.nonces.extend(new_nonces.into_iter());

                    let mut outgoing = vec![CoordinatorSend::ToStorage(
                        CoordinatorToStorageMessage::UpdateState(key.clone()),
                    )];

                    let all_finished = sessions
                        .iter()
                        .all(|session| session.signature_shares.len() == key.frost_key.threshold());

                    if all_finished {
                        let signatures = sessions
                            .iter()
                            .map(|session_progress| {
                                frost.combine_signature_shares(
                                    &xonly_frost_key,
                                    &session_progress.sign_session,
                                    session_progress
                                        .signature_shares
                                        .iter()
                                        .map(|(_, &share)| share)
                                        .collect(),
                                )
                            })
                            .collect();

                        self.state = CoordinatorState::FrostKey {
                            key: key.clone(),
                            awaiting_user: false,
                        };

                        outgoing.push(CoordinatorSend::ToUser(CoordinatorToUserMessage::Signed {
                            signatures,
                        }));
                    }

                    Ok(outgoing)
                }
                _ => Err(Error::coordinator_message_kind(&self.state, &message)),
            },
            _ => Err(Error::coordinator_message_kind(&self.state, &message)),
        }
    }

    pub fn do_keygen(
        &mut self,
        devices: &BTreeSet<DeviceId>,
        threshold: usize,
    ) -> Result<CoordinatorToDeviceMessage, ActionError> {
        if devices.len() < threshold {
            panic!(
                "caller needs to ensure that threshold < devices.len(). Tried {}-of-{}",
                threshold,
                devices.len()
            );
        }
        match self.state {
            CoordinatorState::Registration => {
                self.state = CoordinatorState::KeyGen {
                    shares: devices.iter().map(|&device_id| (device_id, None)).collect(),
                };
                Ok(CoordinatorToDeviceMessage::DoKeyGen {
                    devices: devices.clone(),
                    threshold,
                })
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state.name(),
                action: "do_keygen",
            }),
        }
    }

    pub fn keygen_ack(&mut self, ack: bool) -> Result<Vec<CoordinatorSend>, ActionError> {
        match &mut self.state {
            CoordinatorState::FrostKey { awaiting_user, key } if *awaiting_user == true => {
                match ack {
                    true => {
                        *awaiting_user = false;
                        Ok(vec![CoordinatorSend::ToStorage(
                            CoordinatorToStorageMessage::UpdateState(key.clone()),
                        )])
                    }
                    false => {
                        self.state = CoordinatorState::Registration;
                        Ok(vec![])
                    }
                }
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state.name(),
                action: "keygen_ack",
            }),
        }
    }

    pub fn start_sign(
        &mut self,
        message_to_sign: RequestSignMessage,
        tap_tweak: bool,
        signing_parties: BTreeSet<DeviceId>,
    ) -> Result<(Vec<CoordinatorSend>, CoordinatorToDeviceMessage), StartSignError> {
        match &mut self.state {
            CoordinatorState::FrostKey {
                key,
                awaiting_user: false,
            } => {
                let selected = signing_parties.len();
                if selected < key.frost_key.threshold() {
                    return Err(StartSignError::NotEnoughDevicesSelected {
                        selected,
                        threshold: key.frost_key.threshold(),
                    });
                }

                let messages_to_sign = message_to_sign.clone().message_chunks_to_sign();
                let n_signatures = messages_to_sign.len();

                let signing_nonces = signing_parties
                    .into_iter()
                    .map(|device_id| {
                        let nonces_for_device = key
                            .device_nonces
                            .get_mut(&device_id)
                            .ok_or(StartSignError::UnknownDevice { device_id })?;
                        let index_of_first_nonce = nonces_for_device.counter;
                        let index_of_last_nonce =
                            index_of_first_nonce + nonces_for_device.nonces.len();
                        let nonces = nonces_for_device
                            .nonces
                            .iter()
                            .take(n_signatures)
                            .cloned()
                            .collect::<Vec<_>>();
                        if nonces.len() < n_signatures {
                            return Err(StartSignError::NotEnoughNoncesForDevice {
                                device_id,
                                have: nonces.len(),
                                need: n_signatures,
                            });
                        }
                        let mut remaining = nonces_for_device.nonces.split_off(n_signatures);
                        core::mem::swap(&mut nonces_for_device.nonces, &mut remaining);
                        nonces_for_device.counter += n_signatures;

                        Ok((
                            device_id,
                            (nonces, index_of_first_nonce, index_of_last_nonce),
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;

                let mut xonly_frost_key = key.frost_key.clone().into_xonly_key();
                if tap_tweak {
                    let tweak = bitcoin::util::taproot::TapTweakHash::from_key_and_tweak(
                        XOnlyPublicKey::from_slice(&xonly_frost_key.public_key().to_xonly_bytes())
                            .unwrap(),
                        None,
                    )
                    .to_scalar();
                    xonly_frost_key = xonly_frost_key
                        .tweak(Scalar::<Public, Zero>::from_slice(&tweak.to_be_bytes()).unwrap())
                        .unwrap();
                }
                let sessions = messages_to_sign
                    .iter()
                    .enumerate()
                    .map(|(i, message)| {
                        let b_message = Message::raw(&message[..]);
                        let frost = frost::new_without_nonce_generation::<Sha256>();
                        let indexed_nonces = signing_nonces
                            .iter()
                            .map(|(id, (nonce, _, _))| (id.to_x_coord(), nonce[i]))
                            .collect();
                        let sign_session =
                            frost.start_sign_session(&xonly_frost_key, indexed_nonces, b_message);
                        SignSessionProgress {
                            sign_session,
                            signature_shares: Default::default(),
                        }
                    })
                    .collect();

                let key = key.clone();
                self.state = CoordinatorState::Signing {
                    key: key.clone(),
                    sessions,
                    tap_tweak,
                };
                Ok((
                    vec![CoordinatorSend::ToStorage(
                        CoordinatorToStorageMessage::UpdateState(key),
                    )],
                    CoordinatorToDeviceMessage::RequestSign {
                        message_to_sign: message_to_sign.clone(),
                        nonces: signing_nonces.clone(),
                        tap_tweak,
                    },
                ))
            }
            _ => Err(StartSignError::WrongState {
                in_state: self.state().name(),
            }),
        }
    }

    pub fn state(&self) -> &CoordinatorState {
        &self.state
    }

    pub fn key(&self) -> Option<&CoordinatorFrostKey> {
        match self.state() {
            CoordinatorState::FrostKey {
                key,
                awaiting_user: false,
            } => Some(key),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CoordinatorFrostKey {
    frost_key: FrostKey<Normal>,
    device_nonces: BTreeMap<DeviceId, DeviceNonces>,
}

impl CoordinatorFrostKey {
    pub fn devices(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.device_nonces.keys().cloned()
    }

    pub fn threshold(&self) -> usize {
        self.frost_key.threshold()
    }

    pub fn frost_key(&self) -> &FrostKey<Normal> {
        &self.frost_key
    }
}

#[derive(Clone, Debug)]
pub enum CoordinatorState {
    Registration,
    KeyGen {
        shares: BTreeMap<DeviceId, Option<KeyGenProvideShares>>,
    },
    FrostKey {
        key: CoordinatorFrostKey,
        awaiting_user: bool,
    },
    Signing {
        key: CoordinatorFrostKey,
        sessions: Vec<SignSessionProgress>,
        tap_tweak: bool,
    },
}

#[derive(Clone, Debug)]
pub struct SignSessionProgress {
    sign_session: SignSession,
    signature_shares: BTreeMap<DeviceId, Scalar<Public, Zero>>,
}

impl CoordinatorState {
    pub fn name(&self) -> &'static str {
        match self {
            CoordinatorState::Registration => "Registration",
            CoordinatorState::KeyGen { .. } => "KeyGen",
            CoordinatorState::FrostKey { .. } => "FrostKey",
            CoordinatorState::Signing { .. } => "Signing",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceNonces {
    counter: usize,
    nonces: VecDeque<Nonce>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Hash, Eq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct DeviceId {
    pub pubkey: Point,
}

impl core::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.pubkey)
    }
}

impl DeviceId {
    fn to_x_coord(&self) -> Scalar<Public> {
        let x_coord =
            Scalar::from_hash(Sha256::default().chain_update(self.pubkey.to_bytes())).public();
        x_coord
    }
}

fn gen_pop_message(device_ids: impl IntoIterator<Item = DeviceId>) -> [u8; 32] {
    let mut hasher = Sha256::default().tag(b"frostsnap/pop");
    for id in device_ids {
        hasher.update(&id.pubkey.to_bytes());
    }
    hasher.finalize().into()
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FrostSigner {
    keypair: KeyPair,
    state: SignerState,
    nonce_counter: usize,
}

impl FrostSigner {
    pub fn new_random(rng: &mut impl rand_core::RngCore) -> Self {
        Self::new(KeyPair::<Normal>::new(Scalar::random(rng)))
    }

    pub fn new(keypair: KeyPair) -> Self {
        Self {
            keypair,
            state: SignerState::Registered,
            nonce_counter: 0,
        }
    }

    /// temporary hack until we store multiple keygens
    pub fn clear_state(&mut self) {
        *self = Self::new(self.keypair.clone())
    }

    pub fn keypair(&self) -> &KeyPair {
        &self.keypair
    }

    pub fn device_id(&self) -> DeviceId {
        DeviceId {
            pubkey: self.keypair().public_key(),
        }
    }

    pub fn generate_nonces(
        &self,
        keygen_id: [u8; 32],
        start: usize,
        n: usize,
    ) -> impl Iterator<Item = NonceKeyPair> {
        let mut nonce_rng = derive_nonce_rng! {
            // use Deterministic nonce gen to create our polynomial so we reproduce it later
            nonce_gen => nonce::Deterministic::<Sha256>::default().tag(b"frostsnap/nonces"),
            secret => self.keypair.secret_key(),
            // session id must be unique for each key generation session
            public => [keygen_id],
            seedable_rng => ChaCha20Rng
        };

        nonce_rng.set_word_pos((start * 16) as u128);

        (0..n).map(move |_| NonceKeyPair::random(&mut nonce_rng))
    }

    pub fn state(&self) -> &SignerState {
        &self.state
    }

    pub fn recv_coordinator_message(
        &mut self,
        message: CoordinatorToDeviceMessage,
    ) -> MessageResult<Vec<DeviceSend>> {
        match (&self.state, message.clone()) {
            (
                SignerState::Registered,
                CoordinatorToDeviceMessage::DoKeyGen { devices, threshold },
            ) => {
                if !devices.contains(&self.device_id()) {
                    return Ok(vec![]);
                }
                let frost = frost::new_with_deterministic_nonces::<Sha256>();
                // XXX: Right now now duplicate pubkeys are possible because we only have it in the
                // device id and it's given to us as a BTreeSet.
                let pks = devices
                    .iter()
                    .map(|device| device.pubkey)
                    .collect::<Vec<_>>();
                let mut poly_rng = derive_nonce_rng! {
                    // use Deterministic nonce gen to create our polynomial so we reproduce it later
                    nonce_gen => nonce::Deterministic::<Sha256>::default().tag(b"frostsnap/keygen"),
                    secret => self.keypair.secret_key(),
                    // session id must be unique for each key generation session
                    public => [(threshold as u32).to_be_bytes(), &pks[..]],
                    seedable_rng => ChaCha20Rng
                };
                let scalar_poly = generate_scalar_poly(threshold, &mut poly_rng);
                let mut aux_rand = [0u8; 32];
                poly_rng.fill_bytes(&mut aux_rand);

                let shares = devices
                    .iter()
                    .map(|&device| {
                        let x_coord = device.to_x_coord();
                        let share = frost.create_share(&scalar_poly, x_coord);
                        (
                            device,
                            EncryptedShare::new(device.pubkey, &mut poly_rng, &share),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();

                let pop_message = gen_pop_message(devices.iter().cloned());
                let proof_of_possession =
                    frost.create_proof_of_possession(&scalar_poly, Message::raw(&pop_message));

                let point_poly = frost::to_point_poly(&scalar_poly);
                self.state = SignerState::KeyGen {
                    scalar_poly,
                    devices,
                    threshold,
                    aux_rand,
                };

                let nonces = self
                    .generate_nonces(aux_rand, 0, NONCE_BATCH_SIZE)
                    .map(|nonce| nonce.public())
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("correct length");

                Ok(vec![DeviceSend::ToCoordinator(
                    DeviceToCoordindatorMessage {
                        from: self.device_id(),

                        body: DeviceToCoordinatorBody::KeyGenProvideShares(KeyGenProvideShares {
                            my_poly: point_poly,
                            shares,
                            proof_of_possession,
                            nonces,
                        }),
                    },
                )])
            }
            (
                SignerState::KeyGen {
                    devices, aux_rand, ..
                },
                CoordinatorToDeviceMessage::FinishKeyGen { shares_provided },
            ) => {
                if let Some(device) = devices
                    .iter()
                    .find(|device_id| !shares_provided.contains_key(device_id))
                {
                    return Err(Error::signer_invalid_message(
                        &message,
                        format!("Missing shares from {}", device),
                    ));
                }
                let frost = frost::new_with_deterministic_nonces::<Sha256>();

                let point_polys = shares_provided
                    .iter()
                    .map(|(device_id, share)| (device_id.to_x_coord(), share.my_poly.clone()))
                    .collect();
                let transpose_shares = shares_provided
                    .keys()
                    .map(|device_id_receiver| {
                        Ok((
                            device_id_receiver,
                            shares_provided
                                .iter()
                                .map(|(provider_id, share)| {
                                    Ok((
                                        *provider_id,
                                        (
                                            share.shares.get(device_id_receiver).cloned().ok_or(
                                                Error::signer_invalid_message(
                                                    &message,
                                                    format!(
                                                        "Missing shares for {}",
                                                        device_id_receiver
                                                    ),
                                                ),
                                            )?,
                                            share.proof_of_possession.clone(),
                                        ),
                                    ))
                                })
                                .collect::<Result<BTreeMap<_, _>, _>>()?,
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;

                let my_index = self.device_id().to_x_coord();
                let my_shares = transpose_shares
                    .get(&self.device_id())
                    .expect("this device is part of the keygen")
                    .into_iter()
                    .map(|(provider_id, (encrypted_secret_share, pop))| {
                        (
                            provider_id.to_x_coord(),
                            (
                                encrypted_secret_share.decrypt(self.keypair().secret_key()),
                                pop.clone(),
                            ),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();

                let pop_message = gen_pop_message(devices.iter().cloned());
                let keygen = frost.new_keygen(point_polys).unwrap();

                let (secret_share, frost_key) = frost
                    .finish_keygen(
                        keygen.clone(),
                        my_index,
                        my_shares,
                        Message::raw(&pop_message),
                    )
                    .map_err(|e| Error::signer_invalid_message(&message, format!("{}", e)))?;

                let xpub = frost_key.public_key().to_string();

                self.state = SignerState::FrostKey {
                    key: FrostsnapKey {
                        frost_key,
                        secret_share,
                        aux_rand: *aux_rand,
                    },
                    awaiting_ack: true,
                };

                Ok(vec![DeviceSend::ToUser(DeviceToUserMessage::CheckKeyGen {
                    xpub,
                })])
            }
            (
                SignerState::FrostKey {
                    key,
                    awaiting_ack: false,
                },
                CoordinatorToDeviceMessage::RequestSign {
                    nonces,
                    message_to_sign,
                    tap_tweak,
                },
            ) => {
                let (my_nonces, my_nonce_index, _) = match nonces.get(&self.device_id()) {
                    Some(nonce) => nonce,
                    None => return Ok(Vec::new()),
                };

                let expected_nonces = self
                    .generate_nonces(key.aux_rand, *my_nonce_index, my_nonces.len())
                    .map(|nonce| nonce.public())
                    .collect::<Vec<_>>();
                if expected_nonces != *my_nonces {
                    return Err(Error::signer_invalid_message(
                        &message,
                        "Signing request nonces do not match expected".into(),
                    ));
                }

                if self.nonce_counter > *my_nonce_index {
                    return Err(Error::signer_invalid_message(
                        &message,
                        format!(
                            "Attempt to reuse nonces! Expected nonce >= {} but got {}",
                            self.nonce_counter, my_nonce_index
                        ),
                    ));
                }

                // ⚠ Update nonce counter. Overflow would allow nonce reuse.
                self.nonce_counter = my_nonce_index.saturating_add(my_nonces.len());

                self.state = SignerState::AwaitingSignAck {
                    key: key.clone(),
                    message: message_to_sign.clone(),
                    nonces,
                    tap_tweak,
                };
                Ok(vec![DeviceSend::ToUser(
                    DeviceToUserMessage::SignatureRequest {
                        message_to_sign,
                        tap_tweak,
                    },
                )])
            }
            _ => Err(Error::signer_message_kind(&self.state, &message)),
        }
    }

    pub fn keygen_ack(&mut self, ack: bool) -> Result<Vec<DeviceSend>, ActionError> {
        match &mut self.state {
            SignerState::FrostKey { awaiting_ack, .. } if *awaiting_ack == true => {
                if ack {
                    *awaiting_ack = false;
                    Ok(vec![DeviceSend::ToStorage(
                        message::DeviceToStorageMessage::SaveKey,
                    )])
                } else {
                    self.state = SignerState::Registered;
                    Ok(vec![])
                }
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state.name(),
                action: "keygen_ack",
            }),
        }
    }

    pub fn sign_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match &self.state {
            SignerState::AwaitingSignAck {
                key,
                message,
                nonces,
                tap_tweak,
            } => {
                let messages = message.clone().message_chunks_to_sign();

                let frost = frost::new_with_deterministic_nonces::<Sha256>();
                let (_, my_nonce_index, my_replenish_index) =
                    nonces.get(&self.device_id()).expect("already checked");

                let secret_nonces =
                    self.generate_nonces(key.aux_rand, *my_nonce_index, messages.len());

                let mut signature_shares = vec![];
                let xonly_frost_key = key.frost_key.clone().into_xonly_key();

                let xonly_frost_key = if *tap_tweak {
                    let tweak = bitcoin::util::taproot::TapTweakHash::from_key_and_tweak(
                        XOnlyPublicKey::from_slice(&xonly_frost_key.public_key().to_xonly_bytes())
                            .unwrap(),
                        None,
                    )
                    .to_scalar();
                    xonly_frost_key
                        .tweak(Scalar::<Public, Zero>::from_slice(&tweak.to_be_bytes()).unwrap())
                        .unwrap()
                } else {
                    xonly_frost_key
                };

                for (nonce_index, (message, secret_nonce)) in
                    messages.iter().zip(secret_nonces).enumerate()
                {
                    let nonces_at_index = nonces
                        .into_iter()
                        .map(|(id, (nonces, _, _))| (id.to_x_coord(), nonces[nonce_index]))
                        .collect();

                    let message = Message::raw(&message[..]);
                    let sign_session =
                        frost.start_sign_session(&xonly_frost_key, nonces_at_index, message);

                    let sig_share = frost.sign(
                        &xonly_frost_key,
                        &sign_session,
                        self.device_id().to_x_coord(),
                        &key.secret_share,
                        secret_nonce,
                    );

                    assert!(frost.verify_signature_share(
                        &xonly_frost_key,
                        &sign_session,
                        self.device_id().to_x_coord(),
                        sig_share,
                    ));

                    signature_shares.push(sig_share);
                }

                let replenish_nonces = self
                    .generate_nonces(key.aux_rand, *my_replenish_index, messages.len())
                    .map(|nonce| nonce.public())
                    .collect();

                self.state = SignerState::FrostKey {
                    key: key.clone(),
                    awaiting_ack: false,
                };

                Ok(vec![
                    DeviceSend::ToStorage(message::DeviceToStorageMessage::ExpendNonce),
                    DeviceSend::ToCoordinator(DeviceToCoordindatorMessage {
                        from: self.device_id(),
                        body: {
                            DeviceToCoordinatorBody::SignatureShare {
                                signature_shares,
                                new_nonces: replenish_nonces,
                            }
                        },
                    }),
                ])
            }
            _ => Err(ActionError::WrongState {
                in_state: self.state.name(),
                action: "sign_ack",
            }),
        }
    }

    pub fn frost_key(&self) -> Option<&FrostKey<Normal>> {
        match self.state() {
            SignerState::Registered => None,
            SignerState::KeyGen { .. } => None,
            SignerState::FrostKey { key, .. } => Some(&key.frost_key),
            SignerState::AwaitingSignAck { key, .. } => Some(&key.frost_key),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SignerState {
    Registered,
    KeyGen {
        scalar_poly: Vec<Scalar>,
        devices: BTreeSet<DeviceId>,
        threshold: usize,
        aux_rand: [u8; 32],
    },
    AwaitingSignAck {
        key: FrostsnapKey,
        message: RequestSignMessage,
        nonces: BTreeMap<DeviceId, (Vec<Nonce>, usize, usize)>,
        tap_tweak: bool,
    },
    FrostKey {
        key: FrostsnapKey,
        awaiting_ack: bool,
    },
}

impl SignerState {
    pub fn name(&self) -> &'static str {
        match self {
            SignerState::Registered => "Registered",
            SignerState::KeyGen { .. } => "KeyGen",
            SignerState::AwaitingSignAck { .. } => "AwaitingSignAck",
            SignerState::FrostKey { .. } => "FrostKey",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FrostsnapKey {
    /// The joint key
    pub frost_key: FrostKey<Normal>,
    /// Our secret share of it
    pub secret_share: Scalar,
    /// auxilliary randomness for generating nonces
    pub aux_rand: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The device was not in a state where it could receive a message of that kind
    MessageKind {
        state: &'static str,
        kind: &'static str,
    },
    /// The content of the message was invalid with respect to the state.
    InvalidMessage { kind: &'static str, reason: String },
}

impl Error {
    pub fn coordinator_message_kind(
        state: &CoordinatorState,
        message: &DeviceToCoordindatorMessage,
    ) -> Self {
        Self::MessageKind {
            state: state.name(),
            kind: message.body.kind(),
        }
    }

    pub fn signer_message_kind(state: &SignerState, message: &CoordinatorToDeviceMessage) -> Self {
        Self::MessageKind {
            state: state.name(),
            kind: message.kind(),
        }
    }

    pub fn coordinator_invalid_message(
        message: &DeviceToCoordindatorMessage,
        reason: String,
    ) -> Self {
        Self::InvalidMessage {
            kind: message.body.kind(),
            reason,
        }
    }

    pub fn signer_invalid_message(message: &CoordinatorToDeviceMessage, reason: String) -> Self {
        Self::InvalidMessage {
            kind: message.kind(),
            reason,
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::MessageKind { state, kind } => write!(
                f,
                "Unexpected message of kind {} for this state {}",
                kind, state
            ),
            Error::InvalidMessage { kind, reason } => {
                write!(f, "Invalid message of kind {}: {}", kind, reason)
            }
        }
    }
}

impl Error {
    pub fn gist(&self) -> String {
        match self {
            Error::MessageKind { state, kind } => format!("mk!{} {}", kind, state),
            Error::InvalidMessage { kind, reason } => format!("im!{}: {}", kind, reason),
        }
    }
}

pub type MessageResult<T> = Result<T, Error>;

#[derive(Debug, Clone)]
pub enum DoKeyGenError {
    WrongState,
}

#[derive(Debug, Clone)]
pub enum StartSignError {
    UnknownDevice {
        device_id: DeviceId,
    },
    NotEnoughDevicesSelected {
        selected: usize,
        threshold: usize,
    },
    WrongState {
        in_state: &'static str,
    },
    NotEnoughNoncesForDevice {
        device_id: DeviceId,
        have: usize,
        need: usize,
    },
}

impl core::fmt::Display for StartSignError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StartSignError::NotEnoughDevicesSelected {
                selected,
                threshold,
            } => {
                write!(
                    f,
                    "Need more than {} signers for threshold {}",
                    selected, threshold
                )
            }
            StartSignError::WrongState { in_state } => {
                write!(f, "Can't sign in state {}", in_state)
            }
            StartSignError::NotEnoughNoncesForDevice {
                device_id,
                have,
                need,
            } => {
                write!(
                    f,
                    "Not enough nonces for device {}, have {}, need {}",
                    device_id, have, need,
                )
            }
            StartSignError::UnknownDevice { device_id } => {
                write!(f, "Unknown device {}", device_id)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StartSignError {}

#[derive(Debug, Clone)]
pub enum ActionError {
    WrongState {
        in_state: &'static str,
        action: &'static str,
    },
}

impl core::fmt::Display for ActionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ActionError::WrongState { in_state, action } => {
                write!(f, "Can not {} while in {}", action, in_state)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ActionError {}
