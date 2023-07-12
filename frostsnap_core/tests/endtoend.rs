use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage, DeviceSend,
    DeviceToCoordindatorMessage, DeviceToStorageMessage, DeviceToUserMessage, SignTask,
};
use frostsnap_core::{DeviceId, FrostCoordinator, FrostSigner};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::{frost, fun::marker::Public, Message};
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub enum Send {
    DeviceToUser {
        message: DeviceToUserMessage,
        device_id: DeviceId,
    },
    CoordinatorToUser(CoordinatorToUserMessage),
    DeviceToCoordinator(DeviceToCoordindatorMessage),
    CoordinatorToDevice(CoordinatorToDeviceMessage),
    UserToCoordinator(UserToCoordinator),
    ToStorage, /* ignoring these for now */
}

impl From<CoordinatorSend> for Send {
    fn from(value: CoordinatorSend) -> Self {
        match value {
            CoordinatorSend::ToDevice(v) => v.into(),
            CoordinatorSend::ToUser(v) => v.into(),
            CoordinatorSend::ToStorage(_) => Send::ToStorage,
        }
    }
}

impl From<CoordinatorToUserMessage> for Send {
    fn from(value: CoordinatorToUserMessage) -> Self {
        Send::CoordinatorToUser(value)
    }
}

impl From<DeviceToCoordindatorMessage> for Send {
    fn from(value: DeviceToCoordindatorMessage) -> Self {
        Send::DeviceToCoordinator(value)
    }
}

impl From<CoordinatorToDeviceMessage> for Send {
    fn from(value: CoordinatorToDeviceMessage) -> Self {
        Send::CoordinatorToDevice(value)
    }
}

impl From<DeviceToStorageMessage> for Send {
    fn from(_: DeviceToStorageMessage) -> Self {
        Send::ToStorage
    }
}

#[derive(Debug)]
pub enum UserToCoordinator {
    DoKeyGen {
        threshold: usize,
    },
    StartSign {
        message: SignTask,
        devices: BTreeSet<DeviceId>,
    },
}

#[test]
fn test_end_to_end() {
    let n_parties = 3;
    let threshold = 2;
    let mut coordinator = FrostCoordinator::new();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut devices = (0..n_parties)
        .map(|_| FrostSigner::new_random(&mut test_rng))
        .map(|device| (device.device_id(), device))
        .collect::<BTreeMap<_, _>>();

    let device_id_vec = devices.clone().into_keys().collect::<Vec<_>>();

    // Build a stack of messages last to be processed first
    let mut message_stack: Vec<Send> = vec![];
    // Use select device signers
    // todo use signers bitmask like frost proptest
    let message_to_sign2 = b"johnmcafee47".to_vec();
    let message_to_sign1 = b"pyramid schmee".to_vec();

    message_stack.push(Send::UserToCoordinator(UserToCoordinator::StartSign {
        message: SignTask::Plain(message_to_sign2.clone()),
        devices: BTreeSet::from_iter([device_id_vec[0], device_id_vec[1]]),
    }));
    // Use signers chosen by the coordinator
    message_stack.push(Send::UserToCoordinator(UserToCoordinator::StartSign {
        message: SignTask::Plain(message_to_sign1.clone()),
        devices: BTreeSet::from_iter([device_id_vec[1], device_id_vec[2]]),
    }));
    message_stack.push(Send::UserToCoordinator(UserToCoordinator::DoKeyGen {
        threshold,
    }));

    let mut check_keygens = BTreeMap::default();
    let mut coordinator_check_keygen = None;
    let mut check_sig_requests = BTreeMap::<SignTask, Vec<DeviceId>>::default();
    let mut completed_signature_responses = vec![];
    while !message_stack.is_empty() {
        let to_send = message_stack.pop().unwrap();

        match to_send {
            Send::DeviceToCoordinator(message) => {
                let messages = coordinator.recv_device_message(message).unwrap();
                let messages = messages.into_iter().map(Send::from);
                message_stack.extend(messages);
            }
            Send::CoordinatorToDevice(message) => {
                for destination in devices.keys().cloned().collect::<Vec<_>>() {
                    let device = devices.get_mut(&destination).unwrap();
                    let sends = device.recv_coordinator_message(message.clone()).unwrap();

                    for send in sends {
                        match send {
                            DeviceSend::ToUser(message) => message_stack.push(Send::DeviceToUser {
                                message,
                                device_id: destination,
                            }),
                            DeviceSend::ToCoordinator(message) => {
                                message_stack.push(message.into())
                            }
                            DeviceSend::ToStorage(_) => { /* TODO: test storage */ }
                        }
                    }
                }
            }
            Send::CoordinatorToUser(message) => match message {
                CoordinatorToUserMessage::Signed { signatures } => {
                    completed_signature_responses.push(signatures);
                }
                CoordinatorToUserMessage::CheckKeyGen { xpub } => {
                    coordinator_check_keygen = Some(xpub);
                    coordinator.keygen_ack(true).unwrap();
                }
            },
            Send::UserToCoordinator(message) => {
                let new_messages = match message {
                    UserToCoordinator::DoKeyGen { threshold } => vec![CoordinatorSend::ToDevice(
                        coordinator
                            .do_keygen(&devices.keys().cloned().collect(), threshold)
                            .unwrap(),
                    )],
                    UserToCoordinator::StartSign { message, devices } => {
                        let (mut new_messages, hack) =
                            coordinator.start_sign(message, devices).unwrap();
                        new_messages.push(CoordinatorSend::ToDevice(hack));
                        new_messages
                    }
                };

                message_stack.extend(new_messages.into_iter().map(Send::from));
            }
            Send::DeviceToUser { message, device_id } => match message {
                DeviceToUserMessage::CheckKeyGen { xpub } => {
                    let device = devices.get_mut(&device_id).unwrap();
                    device.keygen_ack(true).unwrap();
                    check_keygens.insert(device_id, xpub);
                }
                DeviceToUserMessage::SignatureRequest { message_to_sign } => {
                    check_sig_requests
                        .entry(message_to_sign.clone())
                        .and_modify(|signers| signers.push(device_id))
                        .or_insert_with(|| vec![device_id]);
                    // Simulate user pressing "sign" --> calls device.sign()
                    let messages = devices.get_mut(&device_id).unwrap().sign_ack(true).unwrap();
                    let messages = messages.into_iter().map(|message| match message {
                        DeviceSend::ToCoordinator(message) => message.into(),
                        DeviceSend::ToUser(message) => Send::DeviceToUser { message, device_id },
                        DeviceSend::ToStorage(m) => m.into(),
                    });
                    message_stack.extend(messages);
                }
            },
            Send::ToStorage => { /* TODO: test storage */ }
        }
    }

    assert_eq!(check_keygens.len(), devices.len());
    let coordinator_check_keygen =
        coordinator_check_keygen.expect("coordinator should have asked user to check keygen");
    for digest in check_keygens.values() {
        assert_eq!(digest, &coordinator_check_keygen);
    }

    assert_eq!(check_sig_requests.len(), 2, "three messages were signed");
    assert!(
        check_sig_requests
            .values()
            .all(|devices| devices.len() == 2),
        "two devices signed each message"
    );
    assert_eq!(completed_signature_responses.len(), 2);

    let frost_key = {
        let mut devices = devices.iter();
        let first = devices.next().unwrap().1;
        for (_, device) in devices {
            assert_eq!(device.frost_key(), first.frost_key());
        }
        first.frost_key().unwrap()
    };

    let frost = frost::new_without_nonce_generation::<Sha256>();

    for (messages, signatures) in vec![
        vec![message_to_sign1.clone()],
        vec![message_to_sign2.clone()],
        vec![message_to_sign1, message_to_sign2],
    ]
    .into_iter()
    .zip(completed_signature_responses)
    {
        for (i, message) in messages.iter().enumerate() {
            let signed_message = Message::<Public>::raw(&message[..]);
            assert!(frost.schnorr.verify(
                &frost_key.clone().into_xonly_key().public_key(),
                signed_message,
                &signatures[i]
            ));
        }
    }
}
