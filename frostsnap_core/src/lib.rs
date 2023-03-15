#![no_std]

pub mod encrypted_share;
pub mod message;
pub mod xpub;

#[macro_use]
extern crate alloc;

use crate::{
    encrypted_share::EncryptedShare,
    message::{
        CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToDeviceSend,
        CoordinatorToUserMessage, DeviceSend, DeviceToCoordindatorMessage, DeviceToUserMessage,
        KeyGenProvideShares,
    },
};
use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    string::String,
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
use xpub::ExtendedPubKey;

#[derive(Debug, Clone)]
pub struct FrostCoordinator {
    state: CoordinatorState,
}

pub const NONCE_BATCH_SIZE: usize = 2;

impl FrostCoordinator {
    pub fn new() -> Self {
        Self {
            state: CoordinatorState::Registration,
        }
    }

    pub fn recv_device_message(
        &mut self,
        message: DeviceToCoordindatorMessage,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        match &mut self.state {
            CoordinatorState::Registration => {
                return match message {
                    DeviceToCoordindatorMessage::Announce { from } => {
                        Ok(vec![CoordinatorSend::ToDevice(CoordinatorToDeviceSend {
                            destination: Some(from),
                            message: CoordinatorToDeviceMessage::AckAnnounce,
                        })])
                    }
                    _ => Err(InvalidState::MessageKind),
                }
            }
            CoordinatorState::KeyGen {
                shares: shares_provided,
            } => match message {
                DeviceToCoordindatorMessage::KeyGenProvideShares(new_shares) => {
                    if let Some(existing) =
                        shares_provided.insert(new_shares.from, Some(new_shares.clone()))
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
                            let keygen_id = frost.keygen_id(&keygen);
                            let pop_message = gen_pop_message(shares_provided.keys().cloned());

                            let frost_key = match frost.finish_keygen_coordinator(keygen, proofs_of_possession, Message::raw(&pop_message)) {
                                Ok(frost_key) => frost_key,
                                Err(_) => todo!("should notify user somehow that everything was fucked and we're canceling it"),
                            };
                            let xpub = ExtendedPubKey::new(frost_key.public_key(), keygen_id);
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

                            self.state = CoordinatorState::FrostKey {
                                frost_key,
                                device_nonces,
                                awaiting_user: true,
                            };
                            Ok(vec![
                                CoordinatorSend::ToDevice(CoordinatorToDeviceSend {
                                    destination: None,
                                    message: CoordinatorToDeviceMessage::FinishKeyGen {
                                        shares_provided: shares_provided.clone(),
                                    },
                                }),
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
                _ => Err(InvalidState::MessageKind),
            },
            CoordinatorState::Signing {
                signature_shares,
                frost_key,
                sign_session,
                device_nonces,
            } => match message {
                DeviceToCoordindatorMessage::SignatureShare {
                    from,
                    signature_share,
                    new_nonces,
                } => {
                    let n_signatures = 1;
                    let frost = frost::new_without_nonce_generation::<Sha256>();

                    let nonce_for_device = device_nonces
                        .get_mut(&from)
                        .ok_or(InvalidState::InvalidMessage)?;

                    if new_nonces.len() != n_signatures {
                        return Err(InvalidState::InvalidMessage);
                    }

                    if sign_session
                        .participants()
                        .find(|x_coord| *x_coord == from.to_x_coord())
                        .is_none()
                    {
                        return Err(InvalidState::InvalidMessage);
                    }

                    // TODO: This message needs to be authenticated
                    nonce_for_device.nonces.extend(new_nonces.into_iter());
                    let xonly_frost_key = frost_key.clone().into_xonly_key();

                    if frost.verify_signature_share(
                        &xonly_frost_key,
                        sign_session,
                        from.to_x_coord(),
                        signature_share,
                    ) {
                        signature_shares.insert(from, signature_share);
                    } else {
                        return Err(InvalidState::InvalidMessage);
                    }

                    if signature_shares.len() == frost_key.threshold() {
                        let signature = frost.combine_signature_shares(
                            &xonly_frost_key,
                            &sign_session,
                            signature_shares.iter().map(|(_, &share)| share).collect(),
                        );

                        self.state = CoordinatorState::FrostKey {
                            frost_key: frost_key.clone(),
                            device_nonces: device_nonces.clone(),
                            awaiting_user: false,
                        };

                        Ok(vec![CoordinatorSend::ToUser(
                            CoordinatorToUserMessage::Signed { signature },
                        )])
                    } else {
                        Ok(vec![])
                    }
                }
                _ => Err(InvalidState::MessageKind),
            },
            _ => Err(InvalidState::MessageKind),
        }
    }

    pub fn do_keygen(
        &mut self,
        devices: &BTreeSet<DeviceId>,
        threshold: usize,
    ) -> Result<Vec<CoordinatorToDeviceSend>, ActionError> {
        if devices.len() < threshold {
            panic!("caller needs to ensure that threshold < divices.len()");
        }
        match self.state {
            CoordinatorState::Registration => {
                self.state = CoordinatorState::KeyGen {
                    shares: devices.iter().map(|&device_id| (device_id, None)).collect(),
                };
                Ok(vec![CoordinatorToDeviceSend {
                    destination: None,
                    message: CoordinatorToDeviceMessage::DoKeyGen {
                        devices: devices.clone(),
                        threshold,
                    },
                }])
            }
            _ => Err(ActionError::WrongState),
        }
    }

    pub fn keygen_ack(&mut self, ack: bool) -> Result<(), ActionError> {
        match &mut self.state {
            CoordinatorState::FrostKey { awaiting_user, .. } if *awaiting_user == true => {
                match ack {
                    true => *awaiting_user = false,
                    false => self.state = CoordinatorState::Registration,
                }
                Ok(())
            }
            _ => Err(ActionError::WrongState),
        }
    }

    pub fn start_sign(
        &mut self,
        message_to_sign: String,
        signing_parties: BTreeSet<DeviceId>,
    ) -> Result<Vec<CoordinatorToDeviceSend>, StartSignError> {
        match &mut self.state {
            CoordinatorState::FrostKey {
                frost_key,
                device_nonces,
                awaiting_user: false,
            } => {
                let selected = signing_parties.len();
                if selected < frost_key.threshold() {
                    return Err(StartSignError::NotEnoughDevicesSelected { selected });
                }

                let n_signatures = 1;

                let signing_nonces = signing_parties
                    .into_iter()
                    .map(|id| {
                        let nonces_for_device = device_nonces
                            .get_mut(&id)
                            .ok_or(StartSignError::NotEnoughNoncesForDevice { deivce_id: id })?;
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
                            return Err(StartSignError::NotEnoughNoncesForDevice { deivce_id: id });
                        }
                        let mut remaining = nonces_for_device.nonces.split_off(n_signatures);
                        core::mem::swap(&mut nonces_for_device.nonces, &mut remaining);
                        nonces_for_device.counter += n_signatures;

                        Ok((id, (nonces, index_of_first_nonce, index_of_last_nonce)))
                    })
                    .collect::<Result<BTreeMap<_, _>, _>>()?;

                let xonly_frost_key = frost_key.clone().into_xonly_key();
                let b_message = Message::plain("frost-device", message_to_sign.as_bytes());
                let frost = frost::new_without_nonce_generation::<Sha256>();
                let indexed_nonces = signing_nonces
                    .iter()
                    .map(|(id, (nonce, _, _))| (id.to_x_coord(), nonce[0]))
                    .collect();
                let sign_session =
                    frost.start_sign_session(&xonly_frost_key, indexed_nonces, b_message);

                self.state = CoordinatorState::Signing {
                    frost_key: frost_key.clone(),
                    sign_session,
                    device_nonces: device_nonces.clone(),
                    signature_shares: BTreeMap::new(),
                };
                Ok(signing_nonces
                    .iter()
                    .map(|(id, _)| CoordinatorToDeviceSend {
                        destination: Some(*id),
                        message: CoordinatorToDeviceMessage::RequestSign {
                            message_to_sign: message_to_sign.clone(),
                            nonces: signing_nonces.clone(),
                        },
                    })
                    .collect())
            }
            _ => Err(StartSignError::WrongState),
        }
    }

    pub fn state(&self) -> &CoordinatorState {
        &self.state
    }
}

#[derive(Clone, Debug)]
pub enum CoordinatorState {
    Registration,
    KeyGen {
        shares: BTreeMap<DeviceId, Option<KeyGenProvideShares>>,
    },
    FrostKey {
        frost_key: FrostKey<Normal>,
        device_nonces: BTreeMap<DeviceId, DeviceNonces>,
        awaiting_user: bool,
    },
    Signing {
        frost_key: FrostKey<Normal>,
        sign_session: SignSession,
        device_nonces: BTreeMap<DeviceId, DeviceNonces>,
        signature_shares: BTreeMap<DeviceId, Scalar<Public, Zero>>,
    },
}

#[derive(Debug, Clone)]
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

impl DeviceId {
    fn to_x_coord(&self) -> Scalar<Public> {
        let x_coord =
            Scalar::from_hash(Sha256::default().chain_update(self.pubkey.to_bytes().as_ref()))
                .public();
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

#[derive(Clone, Debug)]
pub struct FrostSigner {
    keypair: KeyPair,
    state: SignerState,
    nonce_counter: usize,
}

impl FrostSigner {
    pub fn new_random(rng: &mut impl rand_core::RngCore) -> Self {
        Self::new(KeyPair::new(Scalar::random(rng)))
    }

    pub fn new(keypair: KeyPair) -> Self {
        Self {
            keypair,
            state: SignerState::Unregistered,
            nonce_counter: 0,
        }
    }

    pub fn announce(&mut self) -> Option<DeviceToCoordindatorMessage> {
        match self.state {
            SignerState::Unregistered => Some(DeviceToCoordindatorMessage::Announce {
                from: self.device_id(),
            }),
            _ => None,
        }
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
        use CoordinatorToDeviceMessage::*;
        match (&self.state, message) {
            (_, AckAnnounce) => {
                self.state = SignerState::Registered;
                Ok(vec![])
            }
            (SignerState::Registered, DoKeyGen { devices, threshold }) => {
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
                    frost.create_proof_of_possession(Message::raw(&pop_message), &scalar_poly);

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
                    DeviceToCoordindatorMessage::KeyGenProvideShares(KeyGenProvideShares {
                        from: self.device_id(),
                        my_poly: point_poly,
                        shares,
                        proof_of_possession,
                        nonces,
                    }),
                )])
            }
            (
                SignerState::KeyGen {
                    devices, aux_rand, ..
                },
                FinishKeyGen { shares_provided },
            ) => {
                if devices
                    .iter()
                    .any(|device_id| !shares_provided.contains_key(device_id))
                {
                    return Err(InvalidState::InvalidMessage);
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
                                            share
                                                .shares
                                                .get(device_id_receiver)
                                                .cloned()
                                                .ok_or(InvalidState::InvalidMessage)?,
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
                let keygen_id = frost.keygen_id(&keygen);

                let (secret_share, frost_key) = frost
                    .finish_keygen(
                        keygen.clone(),
                        my_index,
                        my_shares,
                        Message::raw(&pop_message),
                    )
                    .map_err(|_e| InvalidState::InvalidMessage)?;

                let xpub = ExtendedPubKey::new(frost_key.public_key(), keygen_id);
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
                RequestSign {
                    nonces,
                    message_to_sign,
                },
            ) => {
                let (my_nonces, my_nonce_index, _) = nonces
                    .get(&self.device_id())
                    .ok_or(InvalidState::InvalidMessage)?;
                if self.nonce_counter > *my_nonce_index {
                    return Err(InvalidState::InvalidMessage);
                }

                let expected_nonces = self
                    .generate_nonces(key.aux_rand, *my_nonce_index, my_nonces.len())
                    .map(|nonce| nonce.public())
                    .collect::<Vec<_>>();
                if expected_nonces != *my_nonces {
                    return Err(InvalidState::InvalidMessage);
                }

                self.state = SignerState::AwaitingSignAck {
                    key: key.clone(),
                    message: message_to_sign.clone(),
                    nonces,
                };
                Ok(vec![DeviceSend::ToUser(
                    DeviceToUserMessage::SignatureRequest { message_to_sign },
                )])
            }
            _ => Err(InvalidState::MessageKind),
        }
    }

    pub fn keygen_ack(&mut self, ack: bool) -> Result<(), ActionError> {
        match &mut self.state {
            SignerState::FrostKey { awaiting_ack, .. } if *awaiting_ack == true => {
                if ack {
                    *awaiting_ack = false;
                } else {
                    self.state = SignerState::Registered;
                }
                Ok(())
            }
            _ => Err(ActionError::WrongState),
        }
    }

    pub fn sign_ack(&mut self) -> Result<Vec<DeviceSend>, ActionError> {
        match &self.state {
            SignerState::AwaitingSignAck {
                key,
                message,
                nonces,
            } => {
                let frost = frost::new_with_deterministic_nonces::<Sha256>();
                let messages = vec![message];
                let (_, my_nonce_index, my_replenish_index) =
                    nonces.get(&self.device_id()).expect("already checked");

                let secret_nonces =
                    self.generate_nonces(key.aux_rand, *my_nonce_index, messages.len());

                let mut signature_shares = vec![];

                for (nonce_index, (message, secret_nonce)) in
                    [message].iter().zip(secret_nonces).enumerate()
                {
                    let nonces_at_index = nonces
                        .into_iter()
                        .map(|(id, (nonces, _, _))| (id.to_x_coord(), nonces[nonce_index]))
                        .collect();
                    let xonly_frost_key = key.frost_key.clone().into_xonly_key();
                    let message = Message::plain("frost-device", message.as_bytes());
                    let sign_session =
                        frost.start_sign_session(&xonly_frost_key, nonces_at_index, message);

                    let sig_share = frost.sign(
                        &xonly_frost_key,
                        &sign_session,
                        self.device_id().to_x_coord(),
                        &key.secret_share,
                        secret_nonce,
                    );
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

                Ok(vec![DeviceSend::ToCoordinator(
                    DeviceToCoordindatorMessage::SignatureShare {
                        signature_share: signature_shares[0],
                        new_nonces: replenish_nonces,
                        from: self.device_id(),
                    },
                )])
            }
            _ => Err(ActionError::WrongState),
        }
    }

    pub fn frost_key(&self) -> Option<&FrostKey<Normal>> {
        match self.state() {
            SignerState::Unregistered => None,
            SignerState::Registered => None,
            SignerState::KeyGen { .. } => None,
            SignerState::FrostKey { key, .. } => Some(&key.frost_key),
            SignerState::AwaitingSignAck { key, .. } => Some(&key.frost_key),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SignerState {
    Unregistered,
    Registered,
    KeyGen {
        scalar_poly: Vec<Scalar>,
        devices: BTreeSet<DeviceId>,
        threshold: usize,
        aux_rand: [u8; 32],
    },
    AwaitingSignAck {
        key: FrostsnapKey,
        message: String,
        nonces: BTreeMap<DeviceId, (Vec<Nonce>, usize, usize)>,
    },
    FrostKey {
        key: FrostsnapKey,
        awaiting_ack: bool,
    },
}

#[derive(Clone, Debug)]
pub struct FrostsnapKey {
    /// The joint key
    pub frost_key: FrostKey<Normal>,
    /// Our secret share of it
    pub secret_share: Scalar,
    /// auxilliary randomness for generating nonces
    pub aux_rand: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidState {
    /// The device was not in a state where it could receive a message of that kind
    MessageKind,
    /// The message received was not valid with respect to the existing state
    InvalidMessage,
}

impl core::fmt::Display for InvalidState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                InvalidState::MessageKind =>
                    "The device was not in a state where it could receive a message of that kind",
                InvalidState::InvalidMessage =>
                    "The message received was not valid with respect to the existing state",
            }
        )
    }
}

pub type MessageResult<T> = Result<T, InvalidState>;

#[derive(Debug, Clone)]
pub enum DoKeyGenError {
    WrongState,
}

#[derive(Debug, Clone)]
pub enum StartSignError {
    NotEnoughDevicesSelected { selected: usize },
    WrongState,
    NotEnoughNoncesForDevice { deivce_id: DeviceId },
}

#[derive(Debug, Clone)]
pub enum ActionError {
    WrongState,
}
