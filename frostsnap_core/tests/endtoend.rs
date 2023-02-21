use frostsnap_core::{
    CoordinatorSend, CoordinatorToDeviceSend, CoordinatorToUserMessage, DeviceId, DeviceSend,
    DeviceToCoordindatorMessage, DeviceToUserMessage, FrostCoordinator, FrostSigner, SignerState,
    UserToCoordinatorMessage,
};
use schnorr_fun::{frost::FrostKey, fun::marker::Normal};
use std::collections::BTreeMap;

#[test]
fn test_end_to_end() {
    let mut coordinator = FrostCoordinator::new();

    let mut devices = (0..3)
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
        CoordinatorToUser(CoordinatorToUserMessage),
        DeviceToCoordinator(DeviceToCoordindatorMessage),
        CoordinatorToDevice(CoordinatorToDeviceSend),
    }

    let mut message_stack = vec![];

    message_stack.push(Send::UserToCoodinator(UserToCoordinatorMessage::DoKeyGen {
        threshold: 2,
    }));

    message_stack.extend(init_messages.into_iter().map(Send::DeviceToCoordinator));

    let mut check_keygens = BTreeMap::<DeviceId, [u8; 32]>::default();
    let mut check_frost_keys = BTreeMap::<DeviceId, FrostKey<Normal>>::default();
    while !message_stack.is_empty() {
        dbg!(&message_stack);
        let to_send = message_stack.pop().unwrap();

        match to_send {
            Send::DeviceToCoordinator(message) => {
                let messages = coordinator.recv_device_message(message);
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
                        .recv_coordinator_message(send.message.clone());

                    for send in sends {
                        match send {
                            DeviceSend::ToUser(message) => match message {
                                DeviceToUserMessage::CheckKeyGen { digest } => {
                                    check_keygens.insert(destination, digest);
                                }
                                DeviceToUserMessage::FinishedFrostKey { frost_key } => {
                                    check_frost_keys.insert(destination, frost_key);
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
                let messages = coordinator.recv_user_message(message);
                let messages = messages.into_iter().map(|message| match message {
                    CoordinatorSend::ToDevice(message) => Send::CoordinatorToDevice(message),
                    CoordinatorSend::ToUser(message) => Send::CoordinatorToUser(message),
                });
                message_stack.extend(messages);
            }
            Send::CoordinatorToUser(_) => {
                todo!()
            }
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
    for key in frost_keys {
        assert_eq!(key, first);
    }
}
