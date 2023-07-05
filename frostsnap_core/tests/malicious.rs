//! Tests for a malicious actions. A malicious coordinator, a malicious device or both.
use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, DeviceSend, KeyGenProvideShares, SignTask,
};
use frostsnap_core::{FrostCoordinator, FrostSigner};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::frost;
use std::collections::BTreeSet;

/// Models a coordinator maliciously replacing a public polynomial contribution and providing a
/// correct share under that malicious polynomial. The device that has had their share replaced
/// should notice it and abort.
#[test]
fn keygen_maliciously_replace_public_poly() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut device = FrostSigner::new_random(&mut test_rng);
    let devices = BTreeSet::from_iter([device.device_id()]);
    let _ = device
        .recv_coordinator_message(CoordinatorToDeviceMessage::DoKeyGen {
            devices: devices.clone(),
            threshold: 1,
        })
        .unwrap();

    let frost = frost::new_with_deterministic_nonces::<sha2::Sha256>();
    let malicious_poly = frost::generate_scalar_poly(1, &mut rand::thread_rng());
    let provide_shares =
        KeyGenProvideShares::generate(&frost, &malicious_poly, &devices, &mut rand::thread_rng());

    let result = device.recv_coordinator_message(CoordinatorToDeviceMessage::FinishKeyGen {
        shares_provided: FromIterator::from_iter([(device.device_id(), provide_shares)]),
    });
    assert!(matches!(
        result,
        Err(frostsnap_core::Error::InvalidMessage { .. })
    ))
}

/// Send the same signing request to a device twice, asking for nonce reuse.
/// The device should reject signing the second request.
#[test]
fn nonce_reuse() {
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
                    device.recv_coordinator_message(message).unwrap();
                }
            }
        }
    }

    coordinator.keygen_ack(true).unwrap();
    device.keygen_ack(true).unwrap();

    let (_coordinator_sends, sign_request) = coordinator
        .start_sign(SignTask::Plain(b"utxo.club!".to_vec()), device_ids.clone())
        .unwrap();

    let _device_responses = device
        .recv_coordinator_message(sign_request.clone())
        .unwrap();

    let _device_sends = device.sign_ack(true).unwrap();

    // Receive a new sign request with the same nonces as the previous session
    let new_sign_request = match sign_request {
        CoordinatorToDeviceMessage::RequestSign { nonces, .. } => {
            CoordinatorToDeviceMessage::RequestSign {
                nonces,
                sign_task: SignTask::Plain(
                    b"we lost track of first FROST txn on bitcoin mainnet @ bushbash 2022".to_vec(),
                ),
            }
        }
        _ => {
            panic!("unreachable");
        }
    };

    let sign_request_result = device.recv_coordinator_message(new_sign_request);

    assert!(matches!(
        sign_request_result,
        Err(frostsnap_core::Error::InvalidMessage { .. })
    ));
    assert!(sign_request_result
        .expect_err("should be error")
        .to_string()
        .contains("Attempt to reuse nonces!"))
}
