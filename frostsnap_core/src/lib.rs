//! Functions for handling communication rounds based on messages, expected peers, acks, and so on
//! Proobably needs rewriting and refactoring into something more robust, though this may depend
//! on the method of DeviceIO used
//!

#![no_std]

#[macro_use]
extern crate alloc;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

use rand_chacha::ChaCha20Rng;
use schnorr_fun::{
    frost::{self, generate_scalar_poly},
    fun::{derive_nonce_rng, marker::*, KeyPair, Point, Scalar},
    nonce,
};
use sha2::digest::Update;
use sha2::Sha256;

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
                _ => todo!("error"),
            },
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
            },
            CoordinatorState::KeyGen { .. } => match message {
                UserToCoordinatorMessage::DoKeyGen { .. } => panic!("We're already doing a keygen"),
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
}

#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq, Ord, PartialOrd)]
pub struct DeviceId {
    pub pubkey: Point,
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
                let frost = frost::new_without_nonce_generation::<Sha256>();
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
                    .filter(|device_id| **device_id != self.device_id())
                    .map(|device| {
                        let x_coord = Scalar::from_hash(
                            Sha256::default().chain(device.pubkey.to_bytes().as_ref()),
                        )
                        .public();
                        frost.create_share(&scalar_poly, x_coord)
                    })
                    .collect();

                let point_poly = frost::to_point_poly(&scalar_poly);
                self.state = SignerState::KeyGen {
                    scalar_poly,
                    devices,
                    threshold,
                };

                vec![DeviceSend::ToCoordinator(
                    DeviceToCoordindatorMessage::KeyGenProvideShares(KeyGenProvideShares {
                        from: self.device_id(),
                        my_poly: point_poly,
                        shares,
                    }),
                )]
            }
            (
                SignerState::KeyGen {
                    scalar_poly,
                    devices,
                    threshold,
                },
                FinishKeyGen { shares_provided },
            ) => {
                vec![]
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
}

#[derive(Clone, Debug)]
pub enum DeviceToCoordindatorMessage {
    Register { device_id: DeviceId },
    KeyGenProvideShares(KeyGenProvideShares),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyGenProvideShares {
    from: DeviceId,
    my_poly: Vec<Point>,
    shares: Vec<Scalar<Secret, Zero>>,
}

#[derive(Clone, Debug)]
pub enum UserToCoordinatorMessage {
    DoKeyGen { threshold: usize },
}

#[derive(Clone, Debug)]
pub enum CoordinatorToUserMessage {}

#[derive(Clone, Debug)]
pub enum DeviceToUserMessage {
    CheckKeyGen { digest: [u8; 32] },
}
