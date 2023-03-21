use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage, DeviceSend,
    DeviceToCoordindatorMessage, DeviceToUserMessage,
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
}

#[derive(Debug)]
pub enum UserToCoordinator {
    DoKeyGen {
        threshold: usize,
    },
    StartSign {
        message: String,
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
    let message_to_sign2 = "johnmcafee47".to_string();
    message_stack.push(Send::UserToCoordinator(UserToCoordinator::StartSign {
        message: message_to_sign2.clone(),
        devices: BTreeSet::from_iter([device_id_vec[0], device_id_vec[1]]),
    }));
    // Use signers chosen by the coordinator
    let message_to_sign = "pyramid schmee".to_string();
    message_stack.push(Send::UserToCoordinator(UserToCoordinator::StartSign {
        message: message_to_sign.clone(),
        devices: BTreeSet::from_iter([device_id_vec[1], device_id_vec[2]]),
    }));
    message_stack.push(Send::UserToCoordinator(UserToCoordinator::DoKeyGen {
        threshold,
    }));

    let mut check_keygens = BTreeMap::default();
    let mut coordinator_check_keygen = None;
    let mut check_sig_requests = BTreeMap::<String, Vec<DeviceId>>::default();
    let mut completed_signature_responses = vec![];
    while !message_stack.is_empty() {
        let to_send = message_stack.pop().unwrap();

        match to_send {
            Send::DeviceToCoordinator(message) => {
                let messages = coordinator.recv_device_message(message).unwrap();
                let messages = messages.into_iter().map(|message| match message {
                    CoordinatorSend::ToDevice(message) => Send::CoordinatorToDevice(message),
                    CoordinatorSend::ToUser(message) => Send::CoordinatorToUser(message),
                });
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
                                message_stack.push(Send::DeviceToCoordinator(message))
                            }
                        }
                    }
                }
            }
            Send::CoordinatorToUser(message) => match message {
                CoordinatorToUserMessage::Signed { signature } => {
                    completed_signature_responses.push(signature);
                }
                CoordinatorToUserMessage::CheckKeyGen { xpub } => {
                    coordinator_check_keygen = Some(xpub);
                    coordinator.keygen_ack(true).unwrap();
                }
            },
            Send::UserToCoordinator(message) => {
                let new_messages = match message {
                    UserToCoordinator::DoKeyGen { threshold } => coordinator
                        .do_keygen(&devices.keys().cloned().collect(), threshold)
                        .unwrap(),
                    UserToCoordinator::StartSign { message, devices } => {
                        coordinator.start_sign(message, devices).unwrap()
                    }
                };

                message_stack.extend(new_messages.into_iter().map(Send::CoordinatorToDevice));
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
                    let messages = devices.get_mut(&device_id).unwrap().sign_ack().unwrap();
                    let messages = messages.into_iter().map(|message| match message {
                        DeviceSend::ToCoordinator(message) => Send::DeviceToCoordinator(message),
                        DeviceSend::ToUser(message) => Send::DeviceToUser { message, device_id },
                    });
                    message_stack.extend(messages);
                }
            },
        }
    }

    assert_eq!(check_keygens.len(), devices.len());
    let coordinator_check_keygen =
        coordinator_check_keygen.expect("coordinator should have asked user to check keygen");
    for digest in check_keygens.values() {
        assert_eq!(digest, &coordinator_check_keygen);
    }

    assert_eq!(check_sig_requests.len(), 2, "two messages were signed");
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
    for (message, signature) in vec![message_to_sign, message_to_sign2]
        .into_iter()
        .zip(completed_signature_responses)
    {
        let signed_message = Message::<Public>::plain("frost-device", message.as_bytes());
        assert!(frost.schnorr.verify(
            &frost_key.clone().into_xonly_key().public_key(),
            signed_message,
            &signature
        ));
    }
}
