//! Functions for handling communication rounds based on messages, expected peers, acks, and so on
//! Proobably needs rewriting and refactoring into something more robust, though this may depend
//! on the method of DeviceIO used
//!

#![no_std]

#[macro_use]
extern crate alloc;

use core::ops::Deref;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

use rand_chacha::ChaCha20Rng;
use schnorr_fun::{
    frost::{self, generate_scalar_poly, FrostKey},
    fun::{derive_nonce_rng, marker::*, KeyPair, Point, Scalar},
    musig::{Nonce, NonceKeyPair},
    nonce, Message, Signature,
};
use sha2::Sha256;
use sha2::{
    digest::{typenum::U32, Update},
    Digest,
};

#[derive(Debug, Clone)]
pub struct FrostCoordinator {
    registered_devices: BTreeSet<DeviceId>,
    state: CoordinatorState,
}

impl FrostCoordinator {
    pub fn new() -> Self {
        Self {
            registered_devices: Default::default(),
            state: CoordinatorState::Registration,
        }
    }

    pub fn recv_device_message(
        &mut self,
        message: DeviceToCoordindatorMessage,
    ) -> Vec<CoordinatorSend> {
        match &mut self.state {
            CoordinatorState::Registration => match message {
                DeviceToCoordindatorMessage::Register { device_id } => {
                    self.registered_devices.insert(device_id);
                    vec![CoordinatorSend::ToDevice(CoordinatorToDeviceSend {
                        destination: Some(device_id),
                        message: CoordinatorToDeviceMessage::RegisterAck {},
                    })]
                }
                _ => todo!("error"),
            },
            CoordinatorState::KeyGen {
                shares: shares_provided,
            } => match message {
                DeviceToCoordindatorMessage::KeyGenProvideShares(new_shares) => {
                    if let Some(existing) =
                        shares_provided.insert(new_shares.from, Some(new_shares.clone()))
                    {
                        if existing != Some(new_shares) && existing.is_some() {
                            todo!("handle different shares for the same device");
                        }
                    }

                    let shares_provided = shares_provided
                        .clone()
                        .into_iter()
                        .map(|(device_id, shares)| Some((device_id, shares?)))
                        .collect::<Option<BTreeMap<_, _>>>();

                    match shares_provided {
                        Some(shares_provided) => {
                            vec![CoordinatorSend::ToDevice(CoordinatorToDeviceSend {
                                destination: None,
                                message: CoordinatorToDeviceMessage::FinishKeyGen {
                                    shares_provided: shares_provided.clone(),
                                },
                            })]
                        }
                        None => vec![],
                    }
                }
                DeviceToCoordindatorMessage::KeyGenFinished {
                    frost_key,
                    initial_nonce,
                    from,
                } => {
                    // Do we want to confirm everyone got the same frost key?
                    let mut nonce_cache = BTreeMap::new();
                    nonce_cache.insert(from, initial_nonce);
                    self.state = CoordinatorState::FrostKey {
                        frost_key,
                        nonce_cache,
                    };
                    vec![]
                }
                _ => todo!("error"),
            },
            CoordinatorState::FrostKey {
                frost_key,
                nonce_cache,
            } => match message {
                DeviceToCoordindatorMessage::KeyGenFinished {
                    frost_key: receieved_frost_key,
                    from,
                    initial_nonce,
                } => {
                    // Update this device's nonce, they probably just finished keygen
                    nonce_cache.insert(from, initial_nonce);
                    assert_eq!(receieved_frost_key, *frost_key.deref());
                    vec![]
                }
                _ => todo!("error"),
            },
            CoordinatorState::Signing {
                message_to_sign,
                nonces,
            } => todo!(),
        }
    }

    pub fn recv_user_message(&mut self, message: UserToCoordinatorMessage) -> Vec<CoordinatorSend> {
        match &self.state {
            CoordinatorState::Registration => match message {
                UserToCoordinatorMessage::DoKeyGen { threshold } => {
                    if threshold > self.registered_devices().len() {
                        panic!("cannot do kegen not enough registered devices");
                    }
                    self.state = CoordinatorState::KeyGen {
                        shares: self
                            .registered_devices()
                            .iter()
                            .map(|&device_id| (device_id, None))
                            .collect(),
                    };
                    vec![CoordinatorSend::ToDevice(CoordinatorToDeviceSend {
                        destination: None,
                        message: CoordinatorToDeviceMessage::DoKeyGen {
                            devices: self.registered_devices.clone(),
                            threshold,
                        },
                    })]
                }
                UserToCoordinatorMessage::StartSign { .. } => {
                    panic!("We can't sign during registration, we don't have a FrostKey yet!")
                }
            },
            CoordinatorState::KeyGen { .. } => match message {
                UserToCoordinatorMessage::DoKeyGen { .. } => panic!("We're already doing a keygen"),
                UserToCoordinatorMessage::StartSign { .. } => {
                    panic!("We can't sign during a keygen")
                }
            },
            CoordinatorState::FrostKey {
                frost_key,
                nonce_cache,
            } => match message {
                UserToCoordinatorMessage::DoKeyGen { .. } => {
                    panic!("We already have a key on this device")
                }
                UserToCoordinatorMessage::StartSign { message_to_sign } => {
                    self.state = CoordinatorState::Signing {
                        nonces: BTreeMap::new(),
                        message_to_sign: message_to_sign.clone(),
                    };
                    vec![CoordinatorSend::ToDevice(CoordinatorToDeviceSend {
                        destination: None,
                        message: CoordinatorToDeviceMessage::SignMessage { message_to_sign },
                    })]
                }
            },
            CoordinatorState::Signing {
                message_to_sign,
                nonces,
            } => match message {
                UserToCoordinatorMessage::DoKeyGen { .. } => {
                    panic!("We already have a key and are signing!")
                }
                UserToCoordinatorMessage::StartSign { .. } => {
                    todo!("We are already in a signing session.")
                }
            },
        }
    }

    pub fn registered_devices(&self) -> &BTreeSet<DeviceId> {
        &self.registered_devices
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
        nonce_cache: BTreeMap<DeviceId, Nonce>,
    },
    Signing {
        message_to_sign: String,
        nonces: BTreeMap<DeviceId, Nonce>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq, Ord, PartialOrd)]
pub struct DeviceId {
    pub pubkey: Point,
}

impl DeviceId {
    fn to_x_coord(&self) -> Scalar<Public> {
        let x_coord =
            Scalar::from_hash(Sha256::default().chain(self.pubkey.to_bytes().as_ref())).public();
        x_coord
    }
}

fn create_keygen_id<H: Digest<OutputSize = U32> + Default + Clone>(
    device_ids: &BTreeSet<DeviceId>,
    hasher: H,
) -> [u8; 32] {
    let mut keygen_hash = hasher;
    keygen_hash.update((device_ids.len() as u32).to_be_bytes());
    for id in device_ids {
        keygen_hash.update(id.pubkey.to_bytes());
    }
    let index_id: [u8; 32] = keygen_hash.finalize().into();
    index_id
}

#[derive(Clone, Debug)]
pub struct FrostSigner {
    keypair: KeyPair,
    state: SignerState,
}

impl FrostSigner {
    pub fn new_random(rng: &mut impl rand_core::RngCore) -> Self {
        Self::new(KeyPair::new(Scalar::random(rng)))
    }

    pub fn new(keypair: KeyPair) -> Self {
        Self {
            keypair,
            state: SignerState::PreRegister,
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

    pub fn init(&self) -> DeviceToCoordindatorMessage {
        DeviceToCoordindatorMessage::Register {
            device_id: DeviceId {
                pubkey: self.keypair.public_key(),
            },
        }
    }

    pub fn state(&self) -> &SignerState {
        &self.state
    }

    pub fn recv_coordinator_message(
        &mut self,
        message: CoordinatorToDeviceMessage,
    ) -> Vec<DeviceSend> {
        use CoordinatorToDeviceMessage::*;
        match (&self.state, message) {
            (_, RegisterAck {}) => {
                self.state = SignerState::Registered;
                vec![]
            }
            (_, DoKeyGen { devices, threshold }) => {
                use schnorr_fun::fun::hash::Tag;
                if !devices.contains(&self.device_id()) {
                    return vec![];
                }
                let frost = frost::new_with_deterministic_nonces::<Sha256>();
                // XXX: Right now now duplicate pubkeys are possible because we only have it in the
                // device id and it's given to us as a BTreeSet.
                let pks = devices
                    .iter()
                    .map(|device| device.pubkey)
                    .collect::<Vec<_>>();
                let mut poly_rng = derive_nonce_rng! {
                    // use Deterministic nonce gen so we reproduce it later
                    nonce_gen => nonce::Deterministic::<Sha256>::default().tag(b"frostsnap/keygen"),
                    secret => self.keypair.secret_key(),
                    // session id must be unique for each key generation session
                    public => [(threshold as u32).to_be_bytes(), &pks[..]],
                    seedable_rng => ChaCha20Rng
                };
                let scalar_poly = generate_scalar_poly(threshold, &mut poly_rng);

                let shares = devices
                    .iter()
                    // // TODO filter ourself?
                    // .filter(|device_id| **device_id != self.device_id())
                    .map(|device| {
                        let x_coord = device.to_x_coord();
                        frost.create_share(&scalar_poly, x_coord)
                    })
                    .collect();

                let keygen_id = create_keygen_id(&devices, Sha256::new());
                let proof_of_possession = frost.create_proof_of_posession(&keygen_id, &scalar_poly);

                let point_poly = frost::to_point_poly(&scalar_poly);
                self.state = SignerState::KeyGen {
                    scalar_poly,
                    devices,
                    threshold,
                };

                vec![
                    DeviceSend::ToCoordinator(DeviceToCoordindatorMessage::KeyGenProvideShares(
                        KeyGenProvideShares {
                            from: self.device_id(),
                            my_poly: point_poly,
                            shares,
                            proof_of_possession,
                        },
                    )),
                    DeviceSend::ToUser(DeviceToUserMessage::CheckKeyGen { digest: keygen_id }),
                ]
            }
            (
                SignerState::KeyGen {
                    scalar_poly,
                    devices,
                    threshold,
                },
                FinishKeyGen { shares_provided },
            ) => {
                let frost = frost::new_with_deterministic_nonces::<Sha256>();

                // Ugly unpack everything according to DeviceID sorting
                let (keygen_device_ids, point_polys, secret_shares, proofs_of_possession) = {
                    let (mut point_polys, mut secret_shares, mut proofs_of_possession) =
                        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
                    for (device_id, share) in shares_provided {
                        point_polys.insert(device_id, share.my_poly);
                        secret_shares.insert(device_id, share.shares);
                        proofs_of_possession.insert(device_id, share.proof_of_possession);
                    }
                    (
                        point_polys.clone().into_keys().collect::<Vec<_>>(),
                        point_polys.into_values().collect::<Vec<_>>(),
                        secret_shares.into_values().collect::<Vec<_>>(),
                        proofs_of_possession.into_values().collect::<Vec<_>>(),
                    )
                };

                let my_index = self.device_id().to_x_coord();
                let device_indexes: Vec<_> =
                    keygen_device_ids.iter().map(|id| id.to_x_coord()).collect();

                let point_polys = device_indexes
                    .into_iter()
                    .zip(point_polys.into_iter())
                    .map(|(index, poly)| (index, poly))
                    .collect();

                // TODO: decrypt our shares
                let positional_index = keygen_device_ids
                    .iter()
                    .position(|&x| x == self.device_id())
                    .unwrap();
                let our_shares = secret_shares
                    .iter()
                    .map(|shares| shares[positional_index].clone())
                    .collect();

                // TODO: check self.devices == keygen_device_ids
                let keygen_id = create_keygen_id(&devices, Sha256::new());
                let keygen = frost.new_keygen(point_polys, keygen_id).unwrap();

                let (secret_share, frost_key) = frost
                    .finish_keygen(
                        keygen.clone(),
                        my_index,
                        our_shares,
                        proofs_of_possession.clone(),
                    )
                    .unwrap();

                // TODO: we might want to store the nonce gen and sid?
                let mut nonce_rng: ChaCha20Rng = frost.seed_nonce_rng(
                    &frost_key,
                    &secret_share,
                    b"this should be extremely unique",
                );

                let initial_nonce = frost.gen_nonce(&mut nonce_rng);
                self.state = SignerState::FrostKey {
                    secret_share,
                    frost_key: frost_key.clone(),
                    next_nonce: initial_nonce.clone(),
                };

                vec![
                    DeviceSend::ToCoordinator(DeviceToCoordindatorMessage::KeyGenFinished {
                        frost_key: frost_key.clone(),
                        initial_nonce: initial_nonce.public(),
                        from: self.device_id(),
                    }),
                    DeviceSend::ToUser(DeviceToUserMessage::FinishedFrostKey { frost_key }),
                ]
            }
            _ => panic!("we received message in unexpected state"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SignerState {
    PreRegister,
    Registered,
    KeyGen {
        scalar_poly: Vec<Scalar>,
        devices: BTreeSet<DeviceId>,
        threshold: usize,
    },
    FrostKey {
        secret_share: Scalar,
        frost_key: FrostKey<Normal>,
        // TODO: Should we allow for a backlog of unused nonces?
        // this is a common pattern with FROST, maybe belongs in the module to be handled securely!
        // See blind signature PR.
        next_nonce: NonceKeyPair,
    },
}

#[derive(Clone, Debug)]
pub enum DeviceSend {
    ToUser(DeviceToUserMessage),
    ToCoordinator(DeviceToCoordindatorMessage),
}

#[derive(Clone, Debug)]
pub enum CoordinatorSend {
    ToDevice(CoordinatorToDeviceSend),
    ToUser(CoordinatorToUserMessage),
}

#[derive(Clone, Debug)]
pub struct CoordinatorToDeviceSend {
    pub destination: Option<DeviceId>,
    pub message: CoordinatorToDeviceMessage,
}

#[derive(Clone, Debug)]
pub enum CoordinatorToDeviceMessage {
    RegisterAck {},
    DoKeyGen {
        devices: BTreeSet<DeviceId>,
        threshold: usize,
    },
    FinishKeyGen {
        shares_provided: BTreeMap<DeviceId, KeyGenProvideShares>,
    },
    SignMessage {
        message_to_sign: String,
    },
}

#[derive(Clone, Debug)]
pub enum DeviceToCoordindatorMessage {
    Register {
        device_id: DeviceId,
    },
    KeyGenProvideShares(KeyGenProvideShares),
    KeyGenFinished {
        from: DeviceId,
        frost_key: FrostKey<Normal>,
        initial_nonce: Nonce,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyGenProvideShares {
    from: DeviceId,
    my_poly: Vec<Point>,
    shares: Vec<Scalar<Secret, Zero>>,
    proof_of_possession: Signature,
}

#[derive(Clone, Debug)]
pub enum UserToCoordinatorMessage {
    DoKeyGen { threshold: usize },
    StartSign { message_to_sign: String },
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {}

#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    CheckKeyGen { digest: [u8; 32] },
    FinishedFrostKey { frost_key: FrostKey<Normal> },
}
