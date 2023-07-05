use frostsnap_core::message::{CoordinatorSend, CoordinatorToDeviceMessage, DeviceSend};
use frostsnap_core::{FrostCoordinator, FrostSigner};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::fun::{g, G};

use std::collections::{BTreeMap, BTreeSet};

mod test_utils;

#[test]
fn wrong_poly() {
    let threshold = 1;
    let mut coordinator = FrostCoordinator::new();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut device = FrostSigner::new_random(&mut test_rng);
    let device_ids = BTreeSet::from_iter([device.device_id()]);

    let do_keygen_message = coordinator.do_keygen(&device_ids, threshold).unwrap();
    let do_keygen_response = device.recv_coordinator_message(do_keygen_message).unwrap();

    for message in do_keygen_response {
        if let DeviceSend::ToCoordinator(message) = message {
            let coordinator_responses = coordinator.recv_device_message(message).unwrap();

            for response in coordinator_responses {
                if let CoordinatorSend::ToDevice(message) = response {
                    if let CoordinatorToDeviceMessage::FinishKeyGen { shares_provided } = message {
                        // Tamper with the poly
                        let mut modified_shares_provided =
                            shares_provided.get(&device.device_id()).unwrap().clone();
                        modified_shares_provided.my_poly = vec![g!(2 * G).normalize()];
                        let keygen_result = device.recv_coordinator_message(
                            CoordinatorToDeviceMessage::FinishKeyGen {
                                shares_provided: BTreeMap::from_iter([(
                                    device.device_id(),
                                    modified_shares_provided,
                                )]),
                            },
                        );

                        // Confirm this gives the correct error
                        match keygen_result {
                            Ok(_) => panic!("This should have been an InvalidMessage poly error!"),
                            Err(err) => match err {
                                frostsnap_core::Error::MessageKind { .. } => {
                                    panic!("This should have been an InvalidMessage poly error!")
                                }
                                frostsnap_core::Error::InvalidMessage { reason, .. } => {
                                    assert_eq!(reason, "Coordinator told us we are using a different point poly than we expected");
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
