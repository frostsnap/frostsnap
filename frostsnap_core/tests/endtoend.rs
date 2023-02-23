use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceSend, CoordinatorToUserMessage, DeviceSend,
    DeviceToCoordindatorMessage, DeviceToUserMessage, UserToCoordinatorMessage,
};
use frostsnap_core::{DeviceId, FrostCoordinator, FrostSigner, SignerState};
use schnorr_fun::{
    frost::{self, FrostKey},
    fun::marker::{Normal, Public},
    Message,
};
use sha2::Sha256;
use std::collections::BTreeMap;

#[test]
fn test_end_to_end() {
    let n_parties = 3;
    let threshold = 2;
    let mut coordinator = FrostCoordinator::new();

    let mut devices = (0..n_parties)
        .map(|_| FrostSigner::new_random(&mut rand::thread_rng()))
        .map(|device| (device.device_id(), device))
        .collect::<BTreeMap<_, _>>();

    let init_messages = devices
        .values()
        .map(|device| device.init())
        .collect::<Vec<_>>();

    #[derive(Debug)]
    pub enum Send {
        UserToCoodinator(UserToCoordinatorMessage),
        DeviceToUser(DeviceToUserMessage),
        CoordinatorToUser(CoordinatorToUserMessage),
        DeviceToCoordinator(DeviceToCoordindatorMessage),
        CoordinatorToDevice(CoordinatorToDeviceSend),
    }

    let device_id_vec = devices.clone().into_keys().collect::<Vec<_>>();

    // Build a stack of messages last to be processed first
    let mut message_stack = vec![];
    // Use select device signers
    // todo use signers bitmask like frost proptest
    let message_to_sign2 = "johnmcafee47".to_string();
    message_stack.push(Send::UserToCoodinator(
        UserToCoordinatorMessage::StartSign {
            message_to_sign: message_to_sign2.clone(),
            signing_parties: vec![device_id_vec[0].clone(), device_id_vec[1].clone()],
        },
    ));
    // Use signers chosen by the coordinator
    let message_to_sign = "pyramid schmee".to_string();
    message_stack.push(Send::UserToCoodinator(
        UserToCoordinatorMessage::StartSign {
            message_to_sign: message_to_sign.clone(),
            signing_parties: vec![device_id_vec[1].clone(), device_id_vec[2].clone()],
        },
    ));
    message_stack.push(Send::UserToCoodinator(UserToCoordinatorMessage::DoKeyGen {
        threshold,
    }));

    message_stack.extend(init_messages.into_iter().map(Send::DeviceToCoordinator));

    let mut check_keygens = BTreeMap::<DeviceId, [u8; 32]>::default();
    let mut check_frost_keys = BTreeMap::<DeviceId, FrostKey<Normal>>::default();
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
            Send::CoordinatorToDevice(send) => {
                let destinations = match send.destination {
                    Some(device) => vec![device],
                    None => devices.keys().cloned().collect(),
                };

                for destination in destinations {
                    let sends = devices
                        .get_mut(&destination)
                        .unwrap()
                        .recv_coordinator_message(send.message.clone())
                        .unwrap();

                    for send in sends {
                        match send {
                            DeviceSend::ToUser(message) => match message {
                                DeviceToUserMessage::CheckKeyGen { digest } => {
                                    check_keygens.insert(destination, digest);
                                }
                                DeviceToUserMessage::FinishedFrostKey { frost_key } => {
                                    check_frost_keys.insert(destination, frost_key);
                                }
                                DeviceToUserMessage::SignatureRequest {
                                    message_to_sign,
                                    nonces,
                                } => {
                                    check_sig_requests
                                        .entry(message_to_sign.clone())
                                        .and_modify(|signers| signers.push(destination))
                                        .or_insert_with(|| vec![destination]);
                                    // Simulate user pressing "sign" --> calls device.sign()
                                    let messages = devices
                                        .get_mut(&destination)
                                        .unwrap()
                                        .sign(message_to_sign.clone(), nonces)
                                        .unwrap();
                                    let messages =
                                        messages.into_iter().map(|message| match message {
                                            DeviceSend::ToCoordinator(message) => {
                                                Send::DeviceToCoordinator(message)
                                            }
                                            DeviceSend::ToUser(message) => {
                                                Send::DeviceToUser(message)
                                            }
                                        });
                                    message_stack.extend(messages);
                                }
                            },
                            DeviceSend::ToCoordinator(message) => {
                                message_stack.push(Send::DeviceToCoordinator(message));
                            }
                        }
                    }
                }
            }
            Send::UserToCoodinator(message) => {
                let messages = coordinator.recv_user_message(message).unwrap();
                let messages = messages.into_iter().map(|message| match message {
                    CoordinatorSend::ToDevice(message) => Send::CoordinatorToDevice(message),
                    CoordinatorSend::ToUser(message) => Send::CoordinatorToUser(message),
                });
                message_stack.extend(messages);
            }
            Send::CoordinatorToUser(message) => match message {
                CoordinatorToUserMessage::Signed { signature } => {
                    completed_signature_responses.push(signature);
                }
            },
            Send::DeviceToUser(_) => todo!(),
        }

        for device in devices.values() {
            match device.state() {
                SignerState::Registered => {
                    assert!(coordinator
                        .registered_devices()
                        .contains(&device.device_id()))
                }
                _ => {}
            }
        }
    }

    assert_eq!(check_keygens.len(), devices.len());
    let mut digests = check_keygens.values();
    let first = digests.next().unwrap();
    for digest in digests {
        assert_eq!(digest, first);
    }

    assert_eq!(check_frost_keys.len(), devices.len());
    let frost_keys = check_frost_keys.values();
    let first = check_frost_keys.values().next().unwrap();
    for key in frost_keys.clone() {
        assert_eq!(key, first);
    }

    assert_eq!(check_sig_requests.len(), 2, "two messages werre signed");
    assert!(
        check_sig_requests
            .values()
            .all(|devices| devices.len() == 2),
        "two devices signed each message"
    );
    assert_eq!(completed_signature_responses.len(), 2);

    let frost_key = frost_keys.collect::<Vec<_>>()[0];
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
